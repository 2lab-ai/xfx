#!/usr/bin/env python3
"""Train 10: a merged file checked from both ends, as rowcheck.py checks a
row: the hunks that take ours' file to the merge are, line for line, the
hunks that took the base's to theirs', and the hunks that take theirs' to the
merge are the hunks that took the base's to ours' (`git diff -U0`, each
hunk's removed and added lines compared; the line numbers are not). One
allowance: an inserted block git slid by blank lines (two blocks inserted at
one place, so the blank line between them can be read as the end of one or the
start of the other) matches its other end's block rotated by those blank lines
alone, and is counted as slid.

    python3 -I -B filecheck.py <repo> <base> <ours> <theirs> <merged|-> <path>...

`-` for merged reads the worktree's file (`git diff` against the worktree).
"""
import subprocess
import sys

repo, base, ours, theirs, merged = sys.argv[1:6]
paths = sys.argv[6:]


def bodies(a, b, path):
    cmd = ["git", "-C", repo, "diff", "-U0", "--no-color", a] + ([b] if b != "-" else []) + ["--", path]
    out, cur = [], None
    for line in subprocess.run(cmd, check=True, capture_output=True, text=True).stdout.split("\n"):
        if line.startswith("@@"):
            cur = []
            out.append(cur)
        elif cur is not None and line[:1] in ("+", "-") and not line.startswith(("+++ ", "--- ")):
            cur.append(line)
    return out


def same(xs, ys):
    """(equal, slid): hunk lists equal, or equal once an insertion block is
    rotated by blank lines only."""
    if xs == ys:
        return True, 0
    if len(xs) != len(ys):
        return False, 0
    slid = 0
    for x, y in zip(xs, ys):
        if x == y:
            continue
        if len(x) != len(y) or any(not l.startswith("+") for l in x + y):
            return False, 0
        ok = False
        for k in range(1, len(x)):
            if (x[k:] + x[:k] == y and all(l[1:].strip() == "" for l in x[:k])) or \
               (y[k:] + y[:k] == x and all(l[1:].strip() == "" for l in y[:k])):
                ok = True
                break
        if not ok:
            return False, 0
        slid += 1
    return True, slid


bad = 0
for path in paths:
    if merged == "-":
        to_m_from_o = bodies(ours, "-", path)
        # theirs -> worktree: diff against theirs' tree
        to_m_from_t = bodies(theirs, "-", path)
    else:
        to_m_from_o = bodies(ours, merged, path)
        to_m_from_t = bodies(theirs, merged, path)
    one, s1 = same(to_m_from_o, bodies(base, theirs, path))
    two, s2 = same(to_m_from_t, bodies(base, ours, path))
    bad += not (one and two)
    print(f"{path}: ours -> merged is theirs' hunks: {one} ({len(to_m_from_o)} hunks, {s1} slid by blank lines); "
          f"theirs -> merged is ours' hunks: {two} ({len(to_m_from_t)} hunks, {s2} slid by blank lines)")
sys.exit(1 if bad else 0)
