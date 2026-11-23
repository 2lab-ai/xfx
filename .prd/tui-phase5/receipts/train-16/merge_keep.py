#!/usr/bin/env python3
# Train 16: train 15's merge_keep.py, unchanged but for this line.
"""Train 15: the dispatcher's merge_keep.py from train 14's review
(reviews/gpt6-merges-1-2.md, "Dispatcher check"), with its normalized form
(digits read as `N`) as an option. For each merge and for docs/parity.md and
the ledger: each side's file split into units -- a table row's cells, each
cell's sentences (after `.` or `;`), the ledger's lines -- of 25 characters
or more; every unit new on a side against the merge base must stand in the
merge result. Prints, per merge, file and side, the count new and the count
missing, and each missing unit (cut at 230 characters).

    python3 -I -B merge_keep.py [--normalize] <repo> <merge>...

A merge may be a probe commit (the staged tree with both parents) as well.
"""
import re
import subprocess
import sys

args = sys.argv[1:]
norm = "--normalize" in args
args = [a for a in args if a != "--normalize"]
repo, merges = args[0], args[1:]


def show(rev, f):
    r = subprocess.run(["git", "-C", repo, "show", f"{rev}:{f}"], capture_output=True, text=True)
    return r.stdout if r.returncode == 0 else ""


def units(text):
    out = set()
    for line in text.split("\n"):
        for cell in re.split(r"\s\|\s", line):
            for s in re.split(r"(?<=[.;])\s+", cell):
                s = s.strip()
                if len(s) >= 25:
                    out.add(re.sub(r"\d+", "N", s) if norm else s)
    return out


total = 0
for m in merges:
    p1, p2 = subprocess.run(["git", "-C", repo, "rev-parse", f"{m}^1", f"{m}^2"], capture_output=True, text=True).stdout.split()
    b = subprocess.run(["git", "-C", repo, "merge-base", p1, p2], capture_output=True, text=True).stdout.strip()
    for f in ["docs/parity.md", "scripts/fxcap/parity-ledger.toml"]:
        B, P1, P2, M = (units(show(r, f)) for r in (b, p1, p2, m))
        for name, P in (("train-side", P1), ("unit-side", P2)):
            miss = sorted((P - B) - M)
            total += len(miss)
            print(f"{m[:8]} {f} {name}: {len(P - B)} new units, missing in merge: {len(miss)}")
            for s in miss[:12]:
                print("    -", s[:230])
print(f"# {'normalized' if norm else 'as written'}: {total} missing over {len(merges)} merge(s)")
