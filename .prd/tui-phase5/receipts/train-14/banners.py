#!/usr/bin/env python3
# Train 14: train 13's banners.py with the banner's form since main's version format of 2026-11-10
# (`x𝒇x v0.2.0 (debug <date>-<rev12>)`) read too, its revision's first seven hex digits compared;
# the old form (`x𝒇x v0.1.0-<rev7>`) is still read.
# Train 13: train 12's banners.py, unchanged but for this line.
"""Each sweep's xfx snaps that show the banner, and how many name the build
swept (`x𝒇x v0.1.0-<rev>`), as train 9 counted them.

    python3 -I -B banners.py <name>=<sweep-out>:<rev7>...
"""
import collections
import glob
import re
import sys

for arg in sys.argv[1:]:
    name, rest = arg.split("=", 1)
    out, rev = rest.rsplit(":", 1)
    total = same = 0
    other = collections.Counter()
    for path in glob.glob(f"{out}/runs/*/xfx/*.txt"):
        text = open(path, errors="replace").read()
        m = re.search(r"x𝒇x v0\.1\.0-([0-9a-f]{7})", text) or re.search(r"x𝒇x v\d+\.\d+\.\d+ \([a-z]+ [0-9-]+-([0-9a-f]{7})[0-9a-f]*\)", text)
        if m:
            total += 1
            same += m.group(1) == rev
            if m.group(1) != rev:
                other[m.group(1)] += 1
    print(f"{name}: {total} xfx snaps show the banner; {same} name the build {rev}; others {dict(other) or 'none'}")
