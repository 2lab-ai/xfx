"""What the change does to the captures besides the status word: each snap's
rows before and after, with `connecting`/`retrying` read as one word and a
loopback port as `<port>`, and every row that still differs.

usage: python3 -I content_diff.py <before sweep out> <after sweep out> <scenario>...
"""
import glob
import os
import re
import sys

before, after, scens = sys.argv[1], sys.argv[2], sys.argv[3:]


def norm(row):
    row = re.sub(r"\b(connecting|retrying)\b", "<state>", row)
    row = re.sub(r"127\.0\.0\.1:\d+", "127.0.0.1:<port>", row)
    return re.sub(r"\s+", " ", row).rstrip()


for scen in scens:
    changed = 0
    for side in ("fx", "xfx"):
        for path in sorted(glob.glob(os.path.join(before, "runs", scen, side, "%s-%s-*.txt" % (side, scen)))):
            other = os.path.join(after, "runs", scen, side, os.path.basename(path))
            label = re.sub(r"^.*-\d+x\d+-", "", os.path.basename(path))[: -len(".txt")]
            if not os.path.isfile(other):
                print("%s %s %s: no capture after" % (scen, label, side))
                continue
            a = [norm(r) for r in open(path, encoding="utf-8").read().split("\n")]
            b = [norm(r) for r in open(other, encoding="utf-8").read().split("\n")]
            rows = [(i + 1, x, y) for i, (x, y) in enumerate(zip(a, b)) if x != y] + (
                [(len(a), "(rows %d)" % len(a), "(rows %d)" % len(b))] if len(a) != len(b) else [])
            if rows:
                changed += 1
                print("%s %s %s: %d row(s) differ, first row %d: %r -> %r" % (scen, label, side, len(rows), rows[0][0], rows[0][1][:70], rows[0][2][:70]))
    print("== %s: %d capture(s) differ beyond the status word and the port" % (scen, changed))
