#!/usr/bin/env python3
"""Train 7's gates_txt.py (train 8's), unchanged but for this line. Writes gates.txt from a gates.sh run: a header, gates.sh's own lines, then
each step's log -- its last lines, or for a test suite every `test result`
line and every failure -- and the reruns named after it, all through
redacted.py (no local path left).

    python3 -I -B gates_txt.py <header-file> <gates.out> <log-dir> <out> [root:<path>=<name>]... [<rerun-title>=<log>]...

A `root:<path>=<name>` argument is a known root redacted.py writes as its
name (the checkout as `<repo>`, the gate's scratch directory as `<scratch>`).
"""
import glob
import os
import re
import subprocess
import sys

header, gates_out, logs, out = sys.argv[1:5]
roots = [a[len("root:"):] for a in sys.argv[5:] if a.startswith("root:")]
reruns = [a.split("=", 1) for a in sys.argv[5:] if not a.startswith("root:")]
here = os.path.dirname(os.path.abspath(__file__))
parts = [open(header).read().rstrip("\n"), open(gates_out).read().rstrip("\n"), ""]


def excerpt(text):
    lines = text.rstrip("\n").split("\n")
    keep = [l for l in lines if l.startswith("test result:") or re.match(r"^(FAIL|ERROR):|^test .* FAILED$|^failures:|AssertionError|panicked at", l)]
    if any(l.startswith("test result:") for l in lines):
        return keep
    return keep + [l for l in lines[-6:] if l not in keep]


for path in sorted(glob.glob(os.path.join(logs, "*.log"))):
    name = os.path.basename(path)[:-4]
    parts.append("===== %s (last lines; test suites: every 'test result' line and every failure)" % name)
    parts += excerpt(open(path, errors="replace").read())
for title, path in reruns:
    parts.append("===== %s" % title)
    parts += excerpt(open(path, errors="replace").read())
text = "\n".join(parts) + "\n"
subprocess.run([sys.executable, "-I", "-B", os.path.join(here, "redacted.py"), out, *roots], input=text, text=True, check=True)
