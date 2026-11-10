"""When each snap of p5-mcp-menu-new was taken, in seconds since the binary's
launch, beside the `remote` status word it shows: where the snaps fall in
the server's try and backoff cycle.

usage: python3 -I timeline.py <repeat out> [<label>...]

The time is each capture's modification time (it is renamed into the out
directory right after the snap, capture.sh `publish`), counted from the
`startup-settled` snap, which follows the `until shown` whose line in
capture.log says how long after launch it ended.
"""
import collections
import glob
import os
import re
import sys

out = sys.argv[1]
want = sys.argv[2:]
SCEN = "p5-mcp-menu-new"
WORD = re.compile(r"\bremote\b.*?\b(connecting|retrying)\b")
SINCE = re.compile(r"until shown 'proj is waiting for approval': after [0-9.]+ s, ([0-9.]+) s since launch")


def word(path):
    with open(path, encoding="utf-8") as f:
        for row in f.read().split("\n"):
            m = WORD.search(row)
            if m:
                return m.group(1)
    return "-"


rows = collections.defaultdict(list)
for d in sorted(glob.glob(os.path.join(out, "*-*"))):
    for side in ("fx", "xfx"):
        run = os.path.join(d, "runs", SCEN, side)
        log = os.path.join(run, "capture.log")
        if not os.path.isfile(log):
            continue
        m = SINCE.search(open(log, encoding="utf-8", errors="replace").read())
        anchor = glob.glob(os.path.join(run, "%s-%s-*-startup-settled.txt" % (side, SCEN)))
        if not m or not anchor:
            continue
        t0 = os.path.getmtime(anchor[0]) - float(m.group(1))
        for path in glob.glob(os.path.join(run, "%s-%s-*.txt" % (side, SCEN))):
            label = re.sub(r"^.*-\d+x\d+-", "", os.path.basename(path))[: -len(".txt")]
            if want and label not in want:
                continue
            rows[label].append((os.path.basename(d), side, os.path.getmtime(path) - t0, word(path)))
for label in sorted(rows, key=lambda k: min(r[2] for r in rows[k])):
    rs = sorted(rows[label], key=lambda r: r[2])
    span = "%.1f-%.1f s" % (rs[0][2], rs[-1][2])
    print("%-22s %-14s %s" % (label, span, "  ".join("%s/%s %.1f %s" % (r[0], r[1], r[2], r[3][:4]) for r in rs)))
