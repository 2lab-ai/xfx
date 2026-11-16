#!/usr/bin/env python3
# Train 16: train 15's tests_kept.py, unchanged but for this line.
"""Train 15 (new): every side's tests are at the head, by name. For each
side (<label>=<base>..<tip>), the test functions it added -- a `fn` right
under a `#[test]` or `#[tokio::test]` line (attributes between allowed), in
a tracked `.rs` file under `src/` or `tests/`, present at the tip and absent
from the base, keyed by (file, name) -- and those it deleted; each added one
looked up at <head> in the same file. Prints per side the counts, every
added test missing at the head, and every test deleted by the side that the
head still holds (it came back) -- then, for each name the head lost that
some side had, which side.

    python3 -I -B tests_kept.py <repo> <head> <label>=<base>..<tip>...
"""
import re
import subprocess
import sys

repo, head, specs = sys.argv[1], sys.argv[2], sys.argv[3:]
ATTR = re.compile(r"^\s*#\[(?:tokio::)?test(?:\(.*\))?\]\s*$")
FN = re.compile(r"^\s*(?:pub(?:\([a-z]+\))? )?(?:async )?fn ([A-Za-z0-9_]+)\b")


def git(*a):
    return subprocess.run(["git", "-C", repo, *a], capture_output=True, text=True).stdout


def tests(commit):
    out = set()
    files = [f for f in git("ls-tree", "-r", "--name-only", commit).split("\n")
             if f.endswith(".rs") and (f.startswith("src/") or f.startswith("tests/"))]
    for f in files:
        armed = False
        for line in git("show", f"{commit}:{f}").split("\n"):
            if ATTR.match(line):
                armed = True
                continue
            if armed and line.strip().startswith("#["):
                continue
            m = FN.match(line)
            if armed and m:
                out.add((f, m.group(1)))
            armed = False
    return out


cache = {}


def at(c):
    if c not in cache:
        cache[c] = tests(c)
    return cache[c]


H = at(head)
bad = 0
for spec in specs:
    label, rng = spec.split("=", 1)
    base, tip = rng.split("..")
    B, T = at(base), at(tip)
    added, deleted = sorted(T - B), sorted(B - T)
    missing = [t for t in added if t not in H]
    back = [t for t in deleted if t in H]
    print(f"{label} ({base[:8]}..{tip[:8]}): {len(added)} tests added, {len(deleted)} deleted; "
          f"added and missing at the head: {len(missing)}; deleted and at the head: {len(back)}")
    for f, n in missing:
        print(f"    missing: {f} {n}")
    for f, n in back:
        print(f"    back: {f} {n}")
    bad += len(missing)
print(f"# head {head[:8]}: {len(H)} tests; {bad} added tests missing")
sys.exit(1 if bad else 0)
