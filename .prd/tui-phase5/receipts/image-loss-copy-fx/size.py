#!/usr/bin/env python3
"""Lines added and removed under src/ outside tests, 714f6283..<rev>: a file's lines from its
top-level `mod tests` on (src/session/store.rs, src/tui/worker.rs, ...) and every *tests.rs are
tests. Positions are the new file's.  usage: python3 -I -B size.py <rev>"""
import re, subprocess, sys
base, rev = "714f6283", sys.argv[1]
files = subprocess.check_output(["git", "diff", "--name-only", base, rev, "--", "src"], text=True).split()
ta = td = 0
for f in files:
    new = subprocess.check_output(["git", "show", f"{rev}:{f}"], text=True).split("\n")
    start = next((i + 1 for i, l in enumerate(new) if re.match(r"^mod tests\b", l)), None)
    if f.endswith("tests.rs"):
        start = 0
    a = d = 0
    at = 0
    for line in subprocess.check_output(["git", "diff", "-U0", base, rev, "--", f], text=True).split("\n"):
        m = re.match(r"@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@", line)
        if m:
            at = int(m.group(1))
            continue
        if line.startswith(("+++", "---")):
            continue
        outside = start is None or at < start
        if line.startswith("+"):
            a += outside
            at += 1
        elif line.startswith("-"):
            d += outside
    print(f"{f}: tests from line {start}: +{a} -{d}")
    ta += a
    td += d
print(f"outside tests: +{ta} -{td} ({ta + td} lines changed, net {ta - td:+d})")
