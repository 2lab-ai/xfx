#!/usr/bin/env python3
"""Lines added and removed between two commits, by class:
  product  -- a src/ .rs line outside the file's `#[cfg(test)] mod tests`;
  tests    -- a src/ .rs line inside it, and every tests/ file;
  receipt  -- every .prd/ file;
  other    -- anything else.
A src/ line's side is read from the line number where `mod tests {` opens
in that version of the file (the old version for a removed line, the new for
an added one). Prints one line per class and per commit-range. Prints no path.

    python3 -I -B numstat.py <repo> <from> <to>
"""
import re
import subprocess
import sys

repo, old_rev, new_rev = sys.argv[1:4]


def git(*args):
    return subprocess.run(["git", "-C", repo, *args], capture_output=True, text=True, check=True).stdout


def tests_from(rev, path):
    try:
        text = git("show", f"{rev}:{path}").split("\n")
    except subprocess.CalledProcessError:
        return None
    for i, line in enumerate(text, 1):
        if line == "mod tests {" and i > 1 and text[i - 2] == "#[cfg(test)]":
            return i
    return None


counts = {}
diff = git("diff", "-U0", "--no-renames", old_rev, new_rev)
path = None
for line in diff.split("\n"):
    if line.startswith("diff --git "):
        path = line.split(" b/", 1)[1]
        old_t, new_t = tests_from(old_rev, path), tests_from(new_rev, path)
        continue
    m = re.match(r"^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@", line)
    if m:
        o, n = int(m.group(1)), int(m.group(2))
        continue
    if line.startswith(("+++", "---")) or not line[:1] in ("+", "-"):
        continue
    if path.startswith(".prd/"):
        cls = "receipt"
    elif path.startswith("tests/"):
        cls = "tests"
    elif path.startswith("src/") and path.endswith(".rs"):
        at, start = (n, new_t) if line[0] == "+" else (o, old_t)
        cls = "tests" if start is not None and at >= start else "product"
    else:
        cls = "other"
    added, removed = counts.setdefault(cls, [0, 0])
    if line[0] == "+":
        counts[cls][0] += 1
        n += 1
    else:
        counts[cls][1] += 1
        o += 1
print(f"{old_rev}..{new_rev}")
for cls in ("product", "tests", "receipt", "other"):
    a, r = counts.get(cls, [0, 0])
    print(f"  {cls:8} +{a} -{r}")
