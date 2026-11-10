"""LOCAL-PATHS: every scripts/fxcap/capture.sh line the parity ledger cites,
with the first characters of the line it names now, so a reader sees that
each citation lands where it means to. ledger.py's own `_sources` checks only
that the file has that many lines; a cite that drifted onto another line
passes it.

usage: python3 -I ledger-refs.py <repo>
"""
import os
import re
import sys

repo = sys.argv[1]
toml = open(os.path.join(repo, "scripts/fxcap/parity-ledger.toml")).read()
cap = open(os.path.join(repo, "scripts/fxcap/capture.sh")).read().split("\n")
rule = None
for line in toml.split("\n"):
    m = re.match(r'id = "([^"]+)"', line)
    if m:
        rule = m.group(1)
    for ref in re.findall(r'"scripts/fxcap/capture\.sh:(\d+)(?:-(\d+))?"', line):
        a = int(ref[0])
        b = int(ref[1] or ref[0])
        for n in (a, b) if b != a else (a,):
            # The whole line: a launch line's TMPDIR= token is past column 70.
            print("%-16s capture.sh:%-5d %s" % (rule, n, cap[n - 1].strip()))
