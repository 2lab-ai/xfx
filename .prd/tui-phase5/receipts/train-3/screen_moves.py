#!/usr/bin/env python3
"""Of the snaps two sweeps share whose screen (the .txt render, row 1 -- the
banner -- left out) is not the same, how many differ only in clock-like text
(dates, hh:mm:ss, durations and latencies, a capture's scratch path, an
activity row's blink mark), masked on both sides; every other one is listed
with its rows.

    python3 -I -B screen_moves.py <reference-sweep-out> <train-sweep-out>
"""
import glob, json, os, re, sys
a_out, b_out = sys.argv[1:3]
MASK = [(re.compile(r"\d{4}-\d{2}-\d{2}"), "D"), (re.compile(r"\d{1,2}:\d{2}:\d{2}(\.\d+)?"), "T"),
        (re.compile(r"\b\d+(\.\d+)?m?s\b"), "S"), (re.compile(r"/[\w./-]*fxcap\.[\w]+[\w./-]*"), "P"),
        (re.compile(r"\b\d+ms\b"), "S"), (re.compile(r"[•◦·∙●○◉ ](?= ?(Thinking|Generating|Working|Running))"), "*")]
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
        other.append((side, sc, label, rows[:6], [(ta[r-2][-60:], tb[r-2][-60:]) for r in rows[:2]]))
print(f"snaps whose screen differs only in clock-like text: {clock}; otherwise: {len(other)}")
for o in other: print("  ", o)
