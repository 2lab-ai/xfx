"""SLASH-MIXED-SKILLS (SLASH-ARG-SKILLS' count.py): the lines of src/ a commit adds and removes against a
base, split into test and non-test -- a line is test when it lies at or past
its file's `#[cfg(test)]` / `mod tests` (this repo's layout) -- and the
non-test lines added that are comments. The brief's split rule reads the
non-test count.

usage: python3 -I count.py <base> <head>   (run from the repo)
"""
import re
import subprocess
import sys


def show(rev, path):
    run = subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True, text=True)
    return run.stdout.split("\n") if run.returncode == 0 else []


def test_start(lines):
    for i, line in enumerate(lines, 1):
        if line.strip() == "#[cfg(test)]" and i < len(lines) and lines[i].strip().startswith("mod tests"):
            return i
    return 10**9


base, head = sys.argv[1], sys.argv[2]
files = subprocess.run(["git", "diff", "--name-only", base, head, "--", "src"],
                       capture_output=True, text=True).stdout.split()
tot = {"src": [0, 0], "test": [0, 0]}
comments = 0
for f in files:
    new_start, old_start = test_start(show(head, f)), test_start(show(base, f))
    old = new = 0
    for line in subprocess.run(["git", "diff", "-U0", base, head, "--", f],
                               capture_output=True, text=True).stdout.split("\n"):
        m = re.match(r"@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@", line)
        if m:
            old, new = int(m.group(1)), int(m.group(2))
            continue
        if line.startswith(("+++", "---")):
            continue
        if line.startswith("+"):
            side = "test" if new >= new_start else "src"
            tot[side][0] += 1
            comments += side == "src" and line[1:].strip().startswith("//")
            new += 1
        elif line.startswith("-"):
            tot["test" if old >= old_start else "src"][1] += 1
            old += 1
    print(f"{f}: tests from line {new_start if new_start < 10**9 else '-'}")
print("non-test +%d -%d (of the added, %d comment lines); test +%d -%d"
      % (tot["src"][0], tot["src"][1], comments, tot["test"][0], tot["test"][1]))
