"""The `remote` server's status word in every snap of every repeat, per binary.

usage: python3 -I words.py <repeat out> <scenario> [<label>...]

For each snap that shows a row naming `remote` with a status word, the word
fx's capture and xfx's capture hold, run by run, and how often they agree.
A row is `remote` followed by a status word (connecting, retrying, failed,
ready, ...) somewhere after it on the same row.
"""
import collections
import glob
import json
import os
import re
import sys

out, scen = sys.argv[1], sys.argv[2]
only = set(sys.argv[3:])
WORD = re.compile(r"\bremote\b.*?\b(connecting|retrying|failed|ready|disabled|unavailable|needs sign-in)\b")


def word(path):
    if not path:
        return "-"
    with open(path[0], encoding="utf-8") as f:
        for row in f.read().split("\n"):
            m = WORD.search(row)
            if m:
                return m.group(1)
    return "-"


def key(d):
    side, i = os.path.basename(d).rsplit("-", 1)
    return (side, int(i))


runs = sorted((p for p in glob.glob(os.path.join(out, "*-*")) if os.path.isfile(os.path.join(p, "pairs.json"))), key=key)
table = collections.OrderedDict()
for d in runs:
    pairs = {p["label"]: p["verdict"] for p in json.load(open(os.path.join(d, "pairs.json"))) if p["scenario"] == scen}
    for label in pairs:
        if only and label not in only:
            continue
        fx = word(glob.glob(os.path.join(d, "runs", scen, "fx", "fx-%s-*-%s.txt" % (scen, label))))
        xf = word(glob.glob(os.path.join(d, "runs", scen, "xfx", "xfx-%s-*-%s.txt" % (scen, label))))
        if fx == "-" and xf == "-":
            continue
        table.setdefault(label, []).append((os.path.basename(d), fx, xf, pairs[label]))
for label, rows in table.items():
    fxw = collections.Counter(r[1] for r in rows)
    xw = collections.Counter(r[2] for r in rows)
    agree = sum(1 for r in rows if r[1] == r[2])
    passed = sum(1 for r in rows if r[3] in ("identical", "normalized-identical"))
    print("%-22s runs %2d  fx %-34s xfx %-34s same word %2d  pass %2d"
          % (label, len(rows), dict(fxw), dict(xw), agree, passed))
print()
for label, rows in table.items():
    print("%-22s %s" % (label, "  ".join("%s:%s/%s" % (r[0], r[1][:4], r[2][:4]) for r in rows)))
