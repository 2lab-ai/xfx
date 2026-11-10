#!/usr/bin/env python3
# Train 13: train 12's overlap.py, unchanged but for this line.
"""For each merge, every file both sides changed since their merge base, each
side's hunks against the base (`git diff -U0`), and the nearest pair of hunks
from the two sides: how many unchanged base lines lie between them (0: they
touch or overlap, which git would have had to resolve).

    python3 -I -B overlap.py <repo> <merge>...
"""
import re
import subprocess
import sys


def git(repo, *args):
    return subprocess.run(["git", "-C", repo, *args], check=True, capture_output=True, text=True).stdout


def spans(repo, base, tip, path):
    out = []
    for line in git(repo, "diff", "-U0", base, tip, "--", path).splitlines():
        m = re.match(r"@@ -(\d+)(?:,(\d+))? \+\d+(?:,\d+)? @@", line)
        if m:
            start, length = int(m.group(1)), int(m.group(2)) if m.group(2) is not None else 1
            # A pure insertion after `start` is the gap [start + 1, start].
            out.append((start + 1, start) if length == 0 else (start, start + length - 1))
    return out


def gap(a, b):
    # Unchanged base lines strictly between two spans (first, last).
    if a[0] > b[0]:
        a, b = b, a
    return max(0, b[0] - a[1] - 1)


def main():
    repo, merges = sys.argv[1], sys.argv[2:]
    for merge in merges:
        ours, theirs = git(repo, "rev-parse", f"{merge}^1", f"{merge}^2").split()
        base = git(repo, "merge-base", ours, theirs).strip()
        a = set(git(repo, "diff", "--name-only", base, ours).splitlines())
        b = set(git(repo, "diff", "--name-only", base, theirs).splitlines())
        both = sorted(a & b)
        print(f"== merge {merge[:8]}: ours {ours[:8]}, theirs {theirs[:8]}, base {base[:8]}; files both changed: {len(both)}")
        for path in both:
            so, st = spans(repo, base, ours, path), spans(repo, base, theirs, path)
            pairs = sorted((gap(x, y), x, y) for x in so for y in st)
            near = pairs[0] if pairs else None
            print(f"  {path}: ours {len(so)} hunks, theirs {len(st)} hunks; nearest: "
                  + (f"{near[0]} unchanged base lines (ours {near[1][0]}-{near[1][1]}, theirs {near[2][0]}-{near[2][1]})"
                     if near else "none"))
            touching = [p for p in pairs if p[0] == 0]
            for g, x, y in touching:
                print(f"    touching: ours {x[0]}-{x[1]}, theirs {y[0]}-{y[1]}")


main()
