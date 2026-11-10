#!/usr/bin/env python3
"""The remote server's word in each take of `p5-mcp-menu-new` `list-remote`.

    python3 -I -B retake_words.py <out dir>...

For each narrow-sweeps.sh output given (group 60), prints the take, the row
naming `remote` on each side's capture (`.txt`), and whether the two say the
same word. The pair's verdict turns on that word alone
(`narrow-pairs.txt`): whichever state the server is in at the snap.
"""
import glob
import os
import re
import sys

ROW = re.compile(r"^› remote\s+(\S+)", re.M)

for out in sys.argv[1:]:
    words = {}
    for side in ("fx", "xfx"):
        found = glob.glob(os.path.join(out, "runs", "p5-mcp-menu-new", side, "*-list-remote.txt"))
        text = open(found[0], encoding="utf-8").read() if found else ""
        m = ROW.search(text)
        words[side] = m.group(1) if m else "(no row)"
    same = "same" if words["fx"] == words["xfx"] else "differ"
    print("%-16s fx %-12s xfx %-12s %s" % (os.path.basename(out), words["fx"], words["xfx"], same))
