"""Splits `git diff <base> <head> -- src/` into the hunks inside each file's
`#[cfg(test)] mod tests` (at head) and the rest, so the tests can be put on
the base alone.

usage: python3 -I split_tests.py <repo> <base> <head> <tests.patch> <prod.patch>
"""
import re
import subprocess
import sys

repo, base, head, tests_out, prod_out = sys.argv[1:6]
diff = subprocess.run(["git", "-C", repo, "diff", "--no-color", base, head, "--", "src/"],
                      check=True, capture_output=True, text=True).stdout
files = re.split(r"(?m)^(?=diff --git )", diff)
tests, prod = [], []
for chunk in files:
    if not chunk.strip():
        continue
    header, *hunks = re.split(r"(?m)^(?=@@ )", chunk)
    path = re.search(r"^\+\+\+ b/(\S+)", header, re.M).group(1)
    text = subprocess.run(["git", "-C", repo, "show", f"{head}:{path}"],
                          check=True, capture_output=True, text=True).stdout
    lines = text.split("\n")
    start = next(i + 1 for i, line in enumerate(lines)
                 if line == "mod tests {" and lines[i - 1] == "#[cfg(test)]")
    t = [h for h in hunks if int(re.match(r"@@ -\d+(?:,\d+)? \+(\d+)", h).group(1)) >= start]
    p = [h for h in hunks if h not in t]
    if t:
        tests.append(header + "".join(t))
    if p:
        prod.append(header + "".join(p))
    print(path, "tests from line", start, "test hunks", len(t), "other hunks", len(p))
open(tests_out, "w").write("".join(tests))
open(prod_out, "w").write("".join(prod))
