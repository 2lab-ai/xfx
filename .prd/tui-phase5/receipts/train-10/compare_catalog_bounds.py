#!/usr/bin/env python3
"""Train 10: the train's sweep against CATALOG-BOUNDS' own, at the verdict
level. The unit's sweep directory is gone (its pairs.json and captures were
in a scratch directory since deleted); what stays is its committed
`sweep-compare.txt` (`e705a5dc:.prd/tui-phase5/receipts/catalog-bounds/`),
which names every pair of its head sweep (`v0.1.0-c9ebc95`, 24 scenarios,
97 pairs) that did not pass -- `## down` and `## fail-both` -- and counts the
rest (`## up`, named; `## pass both`, a count). So a pair of the unit's 24
scenarios passes on the unit unless that file names it failing; this
script reads it so, and lists every such pair that does not pass on the
train's sweep (a drop), and every pair the unit failed that passes on it.
Only pairs both sweeps took count; the unit's scenarios are its `--only`
globs at its sweep's head, c9ebc953.

    python3 -I -B compare_catalog_bounds.py <repo> <train-sweep-out>
"""
import fnmatch
import json
import re
import subprocess
import sys

PASS = ("identical", "normalized-identical")
UNIT = "e705a5dc"
SWEPT_AT = "c9ebc953"
GLOBS = ["p5-catalog-bounds-*", "p5-input-model*", "model*", "h1-models*", "settings-model*"]

repo, out = sys.argv[1:3]
git = lambda *a: subprocess.run(["git", "-C", repo, *a], check=True, capture_output=True, text=True).stdout  # noqa: E731
text = git("show", f"{UNIT}:.prd/tui-phase5/receipts/catalog-bounds/sweep-compare.txt")
section, failing, up, counts = None, set(), set(), {}
for line in text.splitlines():
    m = re.match(r"^## ([a-z -]+): (\d+)$", line)
    if m:
        section = m.group(1)
        counts[section] = int(m.group(2))
        continue
    m = re.match(r"^- (\S+) (\S+): (\S+) -> (\S+)", line)
    if m and section in ("down", "fail-both"):
        failing.add((m.group(1), m.group(2)))
    elif m and section == "up":
        up.add((m.group(1), m.group(2)))
scenarios = sorted(f[len("scripts/fxcap/scenarios/"):-len(".cap")]
                   for f in git("ls-tree", "-r", "--name-only", SWEPT_AT, "scripts/fxcap/scenarios/").split()
                   if f.endswith(".cap") and f.count("/") == 3)
mine = [s for s in scenarios if any(fnmatch.fnmatch(s, g) for g in GLOBS)]
train = {(p["scenario"], p["label"]): p["verdict"] for p in json.load(open(f"{out}/pairs.json"))}
shared = sorted(k for k in train if k[0] in mine)
# The guard: "not named failing" is read as "passed on the unit" only over
# pairs the unit's sweep took. Its 97 are its scenarios' snap labels at
# c9ebc953; a pair the train's sweep takes that the unit's could not have
# (a label added since) is listed and left out.
labels = {}
for s in mine:
    cap = git("show", f"{SWEPT_AT}:scripts/fxcap/scenarios/{s}.cap")
    labels[s] = [l.split()[1] for l in cap.splitlines() if re.match(r"^snap(full)? \S", l)]
unit_pairs = {(s, l) for s in mine for l in labels[s]}
extra = sorted(k for k in shared if k not in unit_pairs)
lost = sorted(k for k in unit_pairs if k not in train)
total = sum(counts.values())
print(f"# the unit's pairs, from its scenarios' snap labels at {SWEPT_AT}: {len(unit_pairs)} "
      f"(its file counts {total}: {'the same' if len(unit_pairs) == total else 'DIFFERENT'})")
print(f"train pairs of those scenarios the unit's sweep did not take: {len(extra)}" + "".join(f"\n    {a} {b}" for a, b in extra))
print(f"unit pairs the train's sweep did not take: {len(lost)}" + "".join(f"\n    {a} {b}" for a, b in lost))
shared = [k for k in shared if k in unit_pairs]
print(f"# CATALOG-BOUNDS' head sweep (its sweep-compare.txt): {sum(counts.values())} pairs "
      f"({', '.join(f'{k} {v}' for k, v in counts.items())}); {len(failing)} named failing; its {len(mine)} scenarios")
print(f"# the train's sweep: {len(shared)} pairs of those scenarios")
missing = sorted((failing | up) - set(shared))
print(f"pairs the unit's file names that the train's sweep did not take: {len(missing)}" + "".join(f"\n    {s} {l}" for s, l in missing))
unit_pass = [k for k in shared if k not in failing]
dropped = [k for k in unit_pass if train[k] not in PASS]
gained = [k for k in shared if k in failing and train[k] in PASS]
print(f"pass, unit -> train: {len(unit_pass)} -> {sum(train[k] in PASS for k in shared)} over {len(shared)} pairs")
print(f"passed on the unit, not on the train: {len(dropped)}" + "".join(f"\n    {s} {l}: {train[(s, l)]}" for s, l in dropped))
print(f"failed on the unit, pass on the train: {len(gained)}" + "".join(f"\n    {s} {l}: {train[(s, l)]}" for s, l in gained))
sys.exit(1 if dropped or len(unit_pairs) != total else 0)
