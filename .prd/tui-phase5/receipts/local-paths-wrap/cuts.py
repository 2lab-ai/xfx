"""What else of the trial's scenario directory a run published: the files of
an e2e.sh out directory that hold its first 24 bytes or more in one piece,
how many of those end at a cut (`…`), and how many hold a shape
scripts/check-no-local-paths.sh refuses in a tracked file (its patterns,
read from the script). Prints counts, never a path.

usage: python3 -I cuts.py <tree> <out> <scenario directory>
"""
import os
import re
import sys

tree, out, scen = sys.argv[1:4]
raw = scen.encode()
src = open(os.path.join(tree, "scripts", "check-no-local-paths.sh"), encoding="utf-8").read()
# The shapes that need no shell expansion: the scratch root and the
# temporary root (the home shapes are built from shell variables there).
shapes = [re.compile(m.encode()) for m in re.findall(r'"a (?:harness scratch root|macOS per-user temporary root):([^"$]+)"', src)]
files = held = cut = shaped = 0
for top, _, names in os.walk(os.path.join(out, "runs")):
    for n in names:
        if n in ("capture.log", "status.json"):
            continue
        files += 1
        data = open(os.path.join(top, n), "rb").read()
        if raw[:24] not in data:
            continue
        held += 1
        cut += bool(re.search(re.escape(raw[:24]) + rb"[A-Za-z0-9._/-]*\xe2\x80\xa6", data))
        shaped += any(s.search(data) for s in shapes)
print("%d published files; %d hold the directory's first 24 bytes or more, %d of them cut by an ellipsis, "
      "%d holding a shape check-no-local-paths.sh refuses (%d shapes read)" % (files, held, cut, shaped, len(shapes)))
