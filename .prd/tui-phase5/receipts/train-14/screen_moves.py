#!/usr/bin/env python3
# Train 14: train 13's screen_moves.py, copied; what changed for this train, if anything, is said below this line.
# Train 13: train 12's screen_moves.py, unchanged but for this line.
"""Of the snaps two sweeps share whose screen (the .txt render, row 1 -- the
banner -- left out) is not the same, how many differ only in clock-like or
run-local text, masked on both sides; every other one is listed with its
rows.

    python3 -I -B screen_moves.py <reference-sweep-out> <train-sweep-out>

Train 3's screen_moves.py with three masks added for train 4: the port of a
run's HTTP MCP server (`127.0.0.1:<port>`, a fresh one each run), the
checkout a sweep ran from (`<anything>/scripts/fxcap/scenarios`, which a
stdio MCP server's command line names, and whose length can move where an
approval wraps), and the resume id a saved session prints. The output is
written through scripts/fxcap/redact.py by the caller.
"""
import glob, json, os, re, sys
# Train 5: a row is redacted whole before its last 60 characters are kept,
# so a cut cannot leave a local path's tail without its root.
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "..", "scripts", "fxcap"))
import redact  # noqa: E402
REDACT = redact.Redactor({})
a_out, b_out = sys.argv[1:3]
MASK = [(re.compile(r"\d{4}-\d{2}-\d{2}"), "D"), (re.compile(r"\d{1,2}:\d{2}:\d{2}(\.\d+)?"), "T"),
        (re.compile(r"\b\d+(\.\d+)?m?s\b"), "S"), (re.compile(r"/[\w./-]*fxcap\.[\w]+[\w./-]*"), "P"),
        (re.compile(r"\b\d+ms\b"), "S"), (re.compile(r"[•◦·∙●○◉ ](?= ?(Thinking|Generating|Working|Running))"), "*"),
        (re.compile(r"(?<=127\.0\.0\.1:)\d+"), "PORT"), (re.compile(r"\S*/scripts/fxcap/scenarios"), "<scenarios>"),
        (re.compile(r"(?<=--resume )[0-9a-f-]{8,}|(?<=resume )[0-9a-f]{8}-[0-9a-f-]+"), "ID")]
def mask(t):
    for r, s in MASK: t = r.sub(s, t)
    return t
pairs = json.load(open(os.path.join(b_out, "pairs.json")))
keys = {(p["scenario"], p["label"]) for p in json.load(open(os.path.join(a_out, "pairs.json")))} & {(p["scenario"], p["label"]) for p in pairs}
clock, other = 0, []
for sc, label in sorted(keys):
    for side in ("fx", "xfx"):
        pat = f"{side}-{sc}-*-{label}.txt"
        fa = glob.glob(os.path.join(a_out, "runs", sc, side, pat)); fb = glob.glob(os.path.join(b_out, "runs", sc, side, pat))
        fa = [f for f in fa if re.search(r"-\d+x\d+-%s\.txt$" % re.escape(label), f)]; fb = [f for f in fb if re.search(r"-\d+x\d+-%s\.txt$" % re.escape(label), f)]
        if len(fa) != 1 or len(fb) != 1: continue
        ta = open(fa[0]).read().split("\n")[1:]; tb = open(fb[0]).read().split("\n")[1:]
        if ta == tb: continue
        if [mask(x) for x in ta] == [mask(x) for x in tb]: clock += 1; continue
        rows = [i + 2 for i, (x, y) in enumerate(zip(ta, tb)) if mask(x) != mask(y)]
        other.append((side, sc, label, rows[:6], [(REDACT(ta[r-2])[-60:], REDACT(tb[r-2])[-60:]) for r in rows[:2]]))
print(f"snaps whose screen differs only in clock-like or run-local text: {clock}; otherwise: {len(other)}")
# Grouped by snap: fx is one binary in both sweeps, so a snap where fx's screen
# moved at the same rows as xfx's moved with the run (the checkout's path
# wrapping a row, a timing), not with xfx's code; a snap where only xfx's
# moved, or xfx's at other rows, is listed in full.
by = {}
for side, sc, label, rows, sample in other:
    by.setdefault((sc, label), {})[side] = (rows, sample)
both = [k for k, v in by.items() if "fx" in v and "xfx" in v and v["fx"][0] == v["xfx"][0]]
print(f"  of them, snaps where fx's screen moved at the same rows as xfx's: {len(both)} snaps")
for k in sorted(both):
    print(f"    {k[0]} {k[1]}: rows {by[k]['fx'][0]}")
rest = sorted(k for k in by if k not in both)
print(f"  snaps where xfx's screen moved alone or at other rows than fx's: {len(rest)}")
for k in rest:
    for side in ("fx", "xfx"):
        if side in by[k]:
            print(f"    {side} {k[0]} {k[1]} rows {by[k][side][0]}: {by[k][side][1]}")
