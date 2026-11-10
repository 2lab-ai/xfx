#!/usr/bin/env python3
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
        m = re.search(r"x𝒇x v0\.1\.0-([0-9a-f]{7})", open(path, errors="replace").read())
        if m:
            total += 1
            same += m.group(1) == rev
            if m.group(1) != rev:
                other[m.group(1)] += 1
    print(f"{name}: {total} xfx snaps show the banner; {same} name v0.1.0-{rev}; others {dict(other) or 'none'}")
