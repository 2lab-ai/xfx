#!/usr/bin/env python3
"""Train 13 (train 10's to 12's filecheck.py, with a second allowance, below):
a merged file checked from both ends, as rowcheck.py checks a
row: the hunks that take ours' file to the merge are, line for line, the
hunks that took the base's to theirs', and the hunks that take theirs' to the
merge are the hunks that took the base's to ours' (`git diff -U0`, each
hunk's removed and added lines compared; the line numbers are not). One
allowance: an inserted block git slid by blank lines (two blocks inserted at
one place, so the blank line between them can be read as the end of one or the
start of the other) matches its other end's block rotated by those blank lines
alone, and is counted as slid.

Train 13's second allowance, tried only when the hunks differ: git's aligner
may place an unchanged line the added text also holds (a `}`, a `);`) on
either side of an insertion, so one end reads `+54 lines, the kept line, +1`
where the other reads `+21, the kept line, +34`. Hunks with at most three
unchanged lines between them are joined into one region, those lines read
from each diff's own old file and counted on both its old and new side; the
two ends then match when every region's old text and new text are equal.
Such a match is counted as joined.

    python3 -I -B filecheck.py [--numbers-masked] <repo> <base> <ours> <theirs> <merged|-> <path>...

`-` for merged reads the worktree's file (`git diff` against the worktree).
`--numbers-masked` compares every line with each `:N` / `:N-M` masked: for a
file whose conflict blocks differed only in reference numbers and were
resolved as ours' (numbers_only.py), so that theirs' re-points are not in the
merge -- the check then says whether that is all that differs.
"""
import re
import subprocess
import sys

MASK = sys.argv[1] == "--numbers-masked"
args = sys.argv[2:] if MASK else sys.argv[1:]
repo, base, ours, theirs, merged = args[:5]
paths = args[5:]
NUMS = re.compile(r":\d+(?:-\d+)?")


def bodies(a, b, path, heads=None):
    cmd = ["git", "-C", repo, "diff", "-U0", "--no-color", a] + ([b] if b != "-" else []) + ["--", path]
    out, cur = [], None
    for line in subprocess.run(cmd, check=True, capture_output=True, text=True).stdout.split("\n"):
        if line.startswith("@@"):
            cur = []
            out.append(cur)
            if heads is not None:
                m = re.match(r"@@ -(\d+)(?:,(\d+))? ", line)
                heads.append((int(m.group(1)), int(m.group(2)) if m.group(2) is not None else 1))
        elif cur is not None and line[:1] in ("+", "-") and not line.startswith(("+++ ", "--- ")):
            cur.append(NUMS.sub(":N", line) if MASK else line)
    return out


def regions(a, b, path, gap=3):
    """Train 13: the diff's hunks joined across at most `gap` unchanged lines,
    each region (old text, new text), the unchanged lines read from `a`."""
    heads = []
    hs = bodies(a, b, path, heads)
    old_file = subprocess.run(["git", "-C", repo, "show", f"{a}:{path}"], check=True, capture_output=True,
                              text=True).stdout.split("\n")
    out = []
    end_prev = None
    for (start, n), body in zip(heads, hs):
        first = start if n > 0 else start + 1  # an insertion is after old line `start`
        minus = [l[1:] for l in body if l.startswith("-")]
        plus = [l[1:] for l in body if l.startswith("+")]
        if out and end_prev is not None and 0 <= first - end_prev <= gap:
            kept = old_file[end_prev - 1:first - 1]
            kept = [NUMS.sub(":N", l) for l in kept] if MASK else kept
            out[-1][0].extend(kept + minus)
            out[-1][1].extend(kept + plus)
        else:
            out.append([minus, plus])
        end_prev = first + n
    return out


def same(xs, ys):
    """(equal, slid): hunk lists equal, or equal once an insertion block is
    rotated by blank lines only."""
    if MASK:
        # Hunks that differ only in numbers vanish once masked: drop them.
        xs = [x for x in xs if [l for l in x if l.startswith("-")] != [l[0].replace("+", "-") + l[1:] for l in x if l.startswith("+")]]
        ys = [y for y in ys if [l for l in y if l.startswith("-")] != [l[0].replace("+", "-") + l[1:] for l in y if l.startswith("+")]]
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
    j1 = j2 = 0
    if not one:
        one = regions(ours, merged, path) == regions(base, theirs, path)
        j1 = int(one)
    if not two:
        two = regions(theirs, merged, path) == regions(base, ours, path)
        j2 = int(two)
    bad += not (one and two)
    print(f"{path}: ours -> merged is theirs' hunks: {one} ({len(to_m_from_o)} hunks, {s1} slid by blank lines"
          f"{', read as joined regions' if j1 else ''}); "
          f"theirs -> merged is ours' hunks: {two} ({len(to_m_from_t)} hunks, {s2} slid by blank lines"
          f"{', read as joined regions' if j2 else ''})")
sys.exit(1 if bad else 0)
