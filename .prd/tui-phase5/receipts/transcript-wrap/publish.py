"""Copies round 2's captures into the receipt, redacted, after checking each
xfx capture's banner names the build it must.

usage: python3 -I -B publish2.py <repo> <cap-dir> <receipt-dir> <green-label> <green-sha7> <red-sha7>
"""
import glob
import os
import re
import shutil
import sys

repo, cap, out = (os.path.realpath(p) for p in sys.argv[1:4])
green, green_sha, red_sha = sys.argv[4:7]
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
from redact import Redactor  # noqa: E402

red = Redactor({repo: "<repo>", cap: "<scratch>"})
UNITS = ["p5-transcript-tools-40x120", "p5-transcript-tools-40x80", "p5-transcript-anchormid-40x120",
         "p5-transcript-anchor-40x120", "p5-transcript-resize-40x120", "p5-transcript-records-40x120",
         "p5-transcript-question-40x120"]
SIDES = {"fx": ("fx", None), green: ("xfx", green_sha), "base": ("red", red_sha)}
BANNER = re.compile(r"^x𝒇x v0\.1\.0-([0-9a-f]{7}) \[debug\]")
for name in ("fx", "xfx", "red", "pairs"):
    shutil.rmtree(os.path.join(out, name), ignore_errors=True)
n = 0
seen = {}
for side, (name, sha) in SIDES.items():
    for unit in UNITS:
        files = sorted(glob.glob(os.path.join(cap, side, unit, "*")))
        assert files, (side, unit)
        for path in files:
            if not path.endswith((".ansi", ".txt")):
                continue
            text = open(path, encoding="utf-8").read()
            if sha and path.endswith(".txt"):
                first = text.split("\n", 1)[0]
                m = BANNER.match(first)
                if m:
                    assert m.group(1) == sha, (path, first, sha)
                    seen[(name, unit)] = seen.get((name, unit), 0) + 1
            dest = os.path.join(out, name, unit)
            os.makedirs(dest, exist_ok=True)
            red.write(os.path.join(dest, os.path.basename(path)), text)
            n += 1
for name in ("xfx", "red"):
    for unit in UNITS:
        assert seen.get((name, unit)), ("no banner seen", name, unit)
        print("%-4s %-34s %2d snaps' row 1 is the banner of %s" % (name, unit, seen[(name, unit)], SIDES[green][1] if name == "xfx" else red_sha))
print("published", n, "files")
