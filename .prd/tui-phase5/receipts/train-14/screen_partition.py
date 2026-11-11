#!/usr/bin/env python3
# Train 14: train 13's screen_partition.py, copied; what changed for this train, if anything, is said below this line.
"""Train 13: compare.py's and screen_moves.py's screen counts tied together.
compare.py counts, per side, the shared snaps whose `.ansi` (row 1 left out)
is not the same in the two sweeps; screen_moves.py counts (side, snap)
screens whose `.txt` (row 1 left out) is not the same, split into those
equal once its masks (clocks, ports, resume ids, ...) are applied and the
rest. This reads both renders of every shared snap, per side, and puts each
moved one in exactly one class:

  txt-masked   the .txt moved, only in screen_moves.py's masked text
  txt-other    the .txt moved beyond the masks (screen_moves.py's listed ones)
  ansi-only    the .ansi moved and the .txt did not (colour or attributes alone)

and says how many of the .txt moves the .ansi does not show (the two
renders are written apart, so a .txt can move where its .ansi did not).

    python3 -I -B screen_partition.py <reference-sweep-out> <train-sweep-out>
"""
import glob
import json
import os
import re
import sys

here = os.path.dirname(os.path.abspath(__file__))
src = open(os.path.join(here, "screen_moves.py"), encoding="utf-8").read()
MASK = eval(re.search(r"^MASK = (\[.*?\])\n", src, re.S | re.M).group(1), {"re": re})


def mask(t):
    for r, s in MASK:
        t = r.sub(s, t)
    return t


def render(out, sc, side, label, ext):
    exact = re.compile(r"^%s-%s-\d+x\d+-%s\.%s$" % (side, re.escape(sc), re.escape(label), ext))
    found = [p for p in glob.glob(os.path.join(out, "runs", sc, side, f"*.{ext}")) if exact.match(os.path.basename(p))]
    return open(found[0], encoding="utf-8").read().split("\n")[1:] if len(found) == 1 else None


a_out, b_out = sys.argv[1:3]
keys = sorted({(p["scenario"], p["label"]) for p in json.load(open(os.path.join(a_out, "pairs.json")))}
              & {(p["scenario"], p["label"]) for p in json.load(open(os.path.join(b_out, "pairs.json")))})
print(f"# {len(keys)} shared snaps")
for side in ("fx", "xfx"):
    cls = {"txt-masked": 0, "txt-other": 0, "ansi-only": 0}
    ansi_moved = unread = txt_not_ansi = 0
    for sc, label in keys:
        aa, ab = render(a_out, sc, side, label, "ansi"), render(b_out, sc, side, label, "ansi")
        ta, tb = render(a_out, sc, side, label, "txt"), render(b_out, sc, side, label, "txt")
        if None in (aa, ab, ta, tb):
            unread += 1
            continue
        ansi_moved += aa != ab
        txt_not_ansi += ta != tb and aa == ab
        if ta != tb:
            cls["txt-masked" if [mask(x) for x in ta] == [mask(x) for x in tb] else "txt-other"] += 1
        elif aa != ab:
            cls["ansi-only"] += 1
    total = sum(cls.values())
    print(f"{side}: .ansi moved {ansi_moved} (compare.py's count); moved, by class: "
          + ", ".join(f"{k} {v}" for k, v in cls.items()) + f"; {total} in all, {txt_not_ansi} of them a .txt move whose .ansi did not move; snaps not read: {unread}")
