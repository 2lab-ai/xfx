#!/usr/bin/env python3
"""Train 16, round 2: what round 2 changed between two code heads, by file
class, so "the product does not change" rests on the lines themselves.
For every hunk line of `git diff -U0 <old> <new>` outside `.prd/`: a line in
a file under `tests/` is a test file's; a line of a `src/` file is test code
when it sits inside that file's `#[cfg(test)] mod tests {` (at or after the
line that opens it, at <old> for a removed line and <new> for an added one),
else a comment when its text, stripped, starts with `//`, else product code.
Prints `git diff --numstat`, then each class's lines out and in.

    python3 -I -B round2_numstat.py <repo> <old> <new>
"""
import re
import subprocess
import sys

repo, old, new = sys.argv[1:4]


def git(*a):
    return subprocess.run(["git", "-C", repo, *a], check=True, capture_output=True, text=True).stdout


def tests_from(commit, path):
    lines = git("show", f"{commit}:{path}").split("\n")
    for i, l in enumerate(lines):
        if l.strip() == "mod tests {" and i > 0 and lines[i - 1].strip() == "#[cfg(test)]":
            return i + 1
    return None


print(f"# git diff --numstat {old[:8]} {new[:8]} -- . ':!.prd'")
print(git("diff", "--numstat", old, new, "--", ".", ":!.prd").rstrip())
counts = {}
for path in git("diff", "--name-only", old, new, "--", ".", ":!.prd").split():
    start = {c: (tests_from(c, path) if path.startswith("src/") else None) for c in (old, new)}
    for h in re.finditer(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@.*\n((?:[-+].*\n?)*)",
                         git("diff", "-U0", old, new, "--", path), re.M):
        a, c = int(h.group(1)), int(h.group(3))
        for line in h.group(5).splitlines():
            side, text = line[0], line[1:]
            at = a if side == "-" else c
            if side == "-":
                a += 1
            else:
                c += 1
            commit = old if side == "-" else new
            if path.startswith("tests/"):
                cls = "test file (tests/)" + (", a comment" if text.strip().startswith("//") else "")
            elif start[commit] and at >= start[commit]:
                cls = f"test code ({path}'s mod tests, from :{start[commit]})"
            elif text.strip().startswith("//"):
                cls = "comment"
            else:
                cls = "product code"
            out, inn = counts.get(cls, (0, 0))
            counts[cls] = (out + (side == "-"), inn + (side == "+"))
print("# by class (lines out, lines in):")
for cls in sorted(counts):
    print(f"  {cls}: -{counts[cls][0]} +{counts[cls][1]}")
print(f"  product code: -{counts.get('product code', (0, 0))[0]} +{counts.get('product code', (0, 0))[1]}"
      if "product code" not in counts else "")
