#!/usr/bin/env python3
# Train 14: train 13's gained_rows.py, copied; what changed for this train, if anything, is said below this line.
# Train 13: train 12's gained_rows.py, unchanged but for this line.
"""Train 12: the pairs compare.py counts as having gained a differing row
(a row pair, fx text and xfx text, the head has and main's sweep had not),
by their verdicts on main and at the head, and by what the new rows hold: one
of capture.sh's placeholders, written where main's rows held a path of the
run (LOCAL-PATHS); a saved session's id; anything else, each listed with
its last 50 characters a side (through the caller's redaction).

    python3 -I -B gained_rows.py <reference-sweep-out> <train-sweep-out>
"""
import collections
import json
import os
import re
import sys


def load(out):
    with open(os.path.join(out, "pairs.json"), encoding="utf-8") as f:
        return {(p["scenario"], p["label"]): p for p in json.load(f)}


a, b = (load(p) for p in sys.argv[1:3])


def rows(p):
    return {(r["fx"], r["xfx"]) for r in p.get("rows", [])}


gained = [k for k in sorted(a) if k in b and rows(b[k]) - rows(a[k])]
print(f"pairs that gained a differing row: {len(gained)}")
for (va, vb), n in sorted(collections.Counter((a[k]["verdict"], b[k]["verdict"]) for k in gained).items()):
    print(f"    main {va} -> head {vb}: {n}")
kinds = collections.Counter()
other = []
for k in gained:
    text = " ".join(f + x for f, x in rows(b[k]) - rows(a[k]))
    if re.search(r"<(scratch|work|tmp|home|scenarios|bun)(:real)?_*>", text) or "_____" in text:
        kinds["a placeholder where main's row held a path of the run (LOCAL-PATHS)"] += 1
    elif re.search(r"\d{13}-[0-9a-f]{16}|--resume|Session  ", text):
        kinds["a saved session's id"] += 1
    else:
        kinds["other"] += 1
        new = sorted(rows(b[k]) - rows(a[k]))[0]
        other.append(f"    {k[0]} {k[1]} ({a[k]['verdict']} -> {b[k]['verdict']}): fx {new[0][-50:]!r} / xfx {new[1][-50:]!r}")
for name, n in kinds.most_common():
    print(f"  {n}  {name}")
print("\n".join(other))
