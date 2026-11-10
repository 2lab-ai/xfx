"""How long PR #90's sweep step needs, from its own log and scoreboard.

usage: python3 -I measure.py <repo> <base rev> <sweep step log> <head scoreboard.json>
                             <head-retry-1 scoreboard.json> <job steps>

The step log is GitHub's, one `<job>\t<step>\t<time> <line>` row each; the job
steps are `gh run view --json jobs` rendered one step a line
(`  <n> <name> | <conclusion> | <started> | <completed>`). Reads the selection
the way ci_sweep.py makes it (its own `select`, `base_labels`), and the
pairs each scenario holds from the head's scoreboard. Prints figures, no path.
"""

import collections
import datetime
import json
import os
import re
import sys

repo, base, log, head_sb, retry_sb, steps = sys.argv[1:7]
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
import capfile  # noqa: E402
import ci_sweep  # noqa: E402

RUN = re.compile(r"^(\S+Z) \[(\d+)/(\d+)\] (fx|xfx) (\S+): (.*) \((\d+)s\)$")
SWEEP = re.compile(r"^(\S+Z) ci-sweep: sweep: (\d+) s$")


def t(stamp):
    return datetime.datetime.fromisoformat(stamp.rstrip("Z")[:26])


lines = []
with open(log, encoding="utf-8-sig") as f:
    for row in f:
        parts = row.rstrip("\n").split("\t")
        if len(parts) >= 3:
            lines.append(parts[2].lstrip("﻿"))

# Each sweep of the step: its runs, from one `ci-sweep: sweep: N scenario(s)`
# line to its `ci-sweep: sweep: <s> s` line.
sweeps, cur = [], None
for line in lines:
    m = re.match(r"^\S+Z ci-sweep: sweep: (\d+) scenario\(s\) with \S+ -> <out>/(\S+)$", line)
    if m:
        cur = {"name": m.group(2), "scenarios": int(m.group(1)), "runs": [], "wall": None}
        sweeps.append(cur)
        continue
    m = RUN.match(line)
    if m and cur is not None:
        cur["runs"].append((m.group(4), m.group(5), m.group(6), int(m.group(7))))
        continue
    m = SWEEP.match(line)
    if m and cur is not None:
        cur["wall"] = int(m.group(2))
print("## The step's sweeps (its log)")
for s in sweeps:
    busy = sum(r[3] for r in s["runs"])
    print("%-14s %3d scenario(s), %3d run(s) logged, run-seconds %5d, wall %s s%s"
          % (s["name"], s["scenarios"], len(s["runs"]), busy, s["wall"],
             "" if s["wall"] is None else ", run-seconds per wall second %.2f" % (busy / s["wall"])))
failed = [r for s in sweeps for r in s["runs"] if not r[2].startswith("ran")]
print("runs not `ran`: %d: %s" % (len(failed), "; ".join("%s %s: %s (%ds)" % r for r in failed)))

head = sweeps[0]
per = collections.defaultdict(int)
for side, name, _, secs in head["runs"]:
    per[name] += secs
inventory = per.pop("(inventory)", 0)
par = sum(r[3] for r in head["runs"]) / head["wall"]

# The selection, as ci_sweep.py makes it.
tree = ci_sweep.Tree(os.path.join(repo, "scripts", "fxcap", "scenarios"))
core = ci_sweep.read_core(os.path.join(repo, "scripts", "fxcap", "ci-core.txt"))
chosen, removed = ci_sweep.select(tree, base, core)
by_reason = collections.Counter(" + ".join(sorted({w.split(" ")[0] for w in why})) for why in chosen.values())
print("\n## The selection (ci_sweep.select at HEAD %s against %s)" % (tree.git("rev-parse", "--short=8", "HEAD").decode().strip(), base[:8]))
print("%d scenario(s); removed %s" % (len(chosen), removed or "none"))
for k, v in by_reason.most_common():
    print("  %-28s %3d" % (k, v))
only_receipt = sorted(n for n, why in chosen.items() if {w.split(" ")[0] for w in why} == {"receipt"})
print("selected by a receipt alone: %d, run-seconds at the head %d (%.0f%% of the head's %d)"
      % (len(only_receipt), sum(per[n] for n in only_receipt), 100.0 * sum(per[n] for n in only_receipt) / sum(per.values()),
         sum(per.values())))

# The base sweep: every scenario that holds a pair the base snaps and the head
# does not pass. The head's per-pair verdicts are not in the artifact (no
# pairs.json); its scoreboard has each scenario's counts. A scenario none of
# whose pairs fail is out; one whose failures outnumber its labels the base
# lacks is surely in; the rest are counted both ways.
sb = json.load(open(head_sb))
retry = json.load(open(retry_sb))
retried = {s["scenario"]: s for s in retry["scenarios"]}
by_name = {u.name: u for u in capfile.units(tree.scenarios)}
surely, maybe, out_new = [], [], []
for s in sb["scenarios"]:
    name = s["scenario"]
    c = s["counts"]
    failing = c["total"] - c["identical"] - c["normalized-identical"]
    if name in retried:  # the retry's verdicts replace the head's for the pairs it re-ran
        rc = retried[name]["counts"]
        failing = min(failing, rc["total"] - rc["identical"] - rc["normalized-identical"])
    if failing == 0:
        continue
    old = ci_sweep.base_labels(tree, base, by_name[name])
    labels = set(by_name[name].labels())
    new = len(labels - (old or set()))
    if old is None:
        out_new.append(name)
    elif failing > new:
        surely.append(name)
    else:
        maybe.append(name)
print("\n## The base sweep it never reached")
print("scenarios holding a pair the head does not pass: %d" % (len(surely) + len(maybe) + len(out_new)))
print("  new at the head (the base has no such scenario, never swept on the base): %d" % len(out_new))
print("  surely swept on the base: %d, run-seconds at the head %d" % (len(surely), sum(per[n] for n in surely)))
print("  maybe (each failing pair could be a label the base lacks): %d, run-seconds %d: %s"
      % (len(maybe), sum(per[n] for n in maybe), " ".join(maybe) or "-"))
lo = sum(per[n] for n in surely) / par + inventory
hi = sum(per[n] for n in surely + maybe) / par + inventory
print("estimated wall at the head's %.2f run-seconds per second, plus fx's inventory run (%d s): %.0f to %.0f s" % (par, inventory, lo, hi))

r1 = sweeps[1]["wall"] if len(sweeps) > 1 and sweeps[1]["wall"] else 0
print("\n## The whole step, estimated")
print("head %d s (measured)" % head["wall"])
print("head retries: retry 1 %d s (measured, %d scenario(s)); retries 2 and 3 at most the same: %d s"
      % (r1, sweeps[1]["scenarios"] if len(sweeps) > 1 else 0, 3 * r1))
print("base: %.0f to %.0f s" % (lo, hi))
# The base's retries: any base-swept scenario whose base diff a live
# nondeterminism cause covers, up to --retries 3. Bounded by every such
# scenario in the base set, each run three more times.
led = ci_sweep.ledgermod.load(os.path.join(repo, "scripts", "fxcap", "parity-ledger.toml"), repo)
nd = [c for c in ci_sweep.ledgermod.live_causes(led) if "nondeterminism" in c["kind"]]
nd_scen = sorted({n for c in nd for n in c["scenarios"]})
in_base = [n for n in nd_scen if n in surely + maybe]
# Those two are the scenarios the head's first retry re-ran, measured at
# 172 s (at 1.51 run-seconds per second, not the head's 1.98), so each base
# retry is taken at that measured time, not extrapolated.
same = len(sweeps) > 1 and set(in_base) == {r[1] for r in sweeps[1]["runs"]} - {"(inventory)"}
bret = 3 * r1 if same else 3 * (sum(per[n] for n in in_base) / par + inventory)
print("live nondeterminism causes: %d, their scenarios %s; of them in the base set: %s"
      % (len(nd), " ".join(nd_scen), " ".join(in_base) or "none"))
print("base retries: 3 x %s: %.0f s" % ("the first head retry's measured %d s (the same scenarios)" % r1 if same else "their run-seconds / %.2f + the inventory run" % par, bret))
step = head["wall"] + 3 * r1 + hi + bret
print("step, estimated: about %.0f s = %.1f min" % (step, step / 60))

pre = None
for line in open(steps):
    m = re.match(r"^\s+(\d+) (.*) \| (\w+) \| (\S+) \| (\S+)$", line.rstrip("\n"))
    if m and m.group(2).startswith("Sweep the changed"):
        started = t(m.group(4))
    m2 = re.match(r"^sweep .* \| \w+ \| \w+ \| (\S+) \| (\S+)$", line.rstrip("\n"))
    if m2:
        job_start = t(m2.group(1))
pre = (started - job_start).total_seconds()
print("the job before the step: %d s (job start to the step's start, the job's steps)" % pre)
print("job, estimated: about %.0f s = %.1f min" % (pre + step, (pre + step) / 60))
