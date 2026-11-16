#!/usr/bin/env python3
"""Each file given, at <rev> and in the worktree: the same number of lines,
and every line that differs differs only in its numbers (each run of digits
masked). Prints one line per file and the sums; exits 1 otherwise.

    python3 -I -B numbers_only.py <repo> <rev> <file>...
Prints no path but the repo-relative files'.
"""
import re
import subprocess
import sys

repo, rev, files = sys.argv[1], sys.argv[2], sys.argv[3:]
mask = lambda s: re.sub(r"\d+", "N", s)
bad = total = 0
for f in files:
    old = subprocess.run(["git", "-C", repo, "show", f"{rev}:{f}"], capture_output=True, text=True,
                         check=True).stdout.split("\n")
    new = open(f"{repo}/{f}", encoding="utf-8").read().split("\n")
    changed = [i for i, (a, b) in enumerate(zip(old, new), 1) if a != b]
    other = [i for i in changed if mask(old[i - 1]) != mask(new[i - 1])]
    same_length = len(old) == len(new)
    total += len(changed)
    bad += bool(other) or not same_length
    print(f"{f}: {len(changed)} lines changed, numbers only: "
          f"{'yes' if not other and same_length else 'NO ' + str(other) + ('' if same_length else ' (line count differs)')}")
print(f"# {len(files)} files, {total} lines changed, {bad} with more than numbers")
sys.exit(1 if bad else 0)
