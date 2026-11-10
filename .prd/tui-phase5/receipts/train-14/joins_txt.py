#!/usr/bin/env python3
"""Train 14 (new): join checks 1 and 2 read from joins.sh's runs.

Part 1, for each TMPDIR run (`j1-24`, `j1-49`): every pair's verdict
(pairs.json), side by side with the head's main sweep's verdict of the same
pair (<sw-head>, the same tree's main sweep at the default TMPDIR), each pair that differs listed; then the
redaction: in every capture `.txt` and `.ansi` of the run, the count of
lines holding the run's TMPDIR (given, never printed), of lines the brief's
path pattern matches (spelt with bracket classes, so this file does not
match itself), and of lines holding a `<scratch` or `<tmp` placeholder, and `workspace-index`'s `at-plan` row 13 on each
side, the `@` list's absolute path, as captured (the placeholder; its width
the path's).

Part 2, for each walk run (`j2-1` .. `j2-5`), when there are any (the
base's runs have none): walk_rows.py's line.

    python3 -I -B joins_txt.py <joins-dir> <sw-head> <tmpdir-24> <tmpdir-49>
"""
import glob
import json
import os
import re
import subprocess
import sys

BRIEF = re.compile(r"/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|zhugehyu[k]|claud[e]-[0-9]+/|-[0-9]{3}/[t][0-9]+/|fx[s][w]\.[A-Za-z0-9_]{6}")

joins, head, tmp24, tmp49 = sys.argv[1:5]
here = os.path.dirname(os.path.abspath(__file__))


def pairs(out):
    with open(os.path.join(out, "pairs.json"), encoding="utf-8") as f:
        return {(p["scenario"], p["label"]): p["verdict"] for p in json.load(f)}


PASS = ("identical", "normalized-identical")
hp = pairs(head)
print("## join check 1: the three globs at a 24-byte and a 49-byte TMPDIR, against the head's main sweep (its own TMPDIR)")
for size, tmp in (("24", tmp24), ("49", tmp49)):
    out = os.path.join(joins, f"j1-{size}")
    jp = pairs(out)
    shared = sorted(set(jp) & set(hp))
    moved = [k for k in shared if jp[k] != hp[k]]
    npass = sum(jp[k] in PASS for k in jp)
    print(f"### TMPDIR of {size} bytes: {len(jp)} pairs, {npass} pass; {len(shared)} also in the head's sweep, "
          f"{sum(hp[k] in PASS for k in shared)} of them pass there; verdicts that differ: {len(moved)}; "
          f"pairs not in the head's sweep: {len(set(jp) - set(hp))}")
    for k in sorted(jp):
        mark = "" if k not in hp else ("" if jp[k] == hp[k] else f"   (head sweep: {hp[k]})")
        print(f"  {k[0]} {k[1]}: {jp[k]}{mark}")
    held = brief = holders = 0
    files = glob.glob(os.path.join(out, "runs", "*", "*", "*.txt")) + glob.glob(os.path.join(out, "runs", "*", "*", "*.ansi"))
    for path in files:
        text = open(path, encoding="utf-8", errors="replace").read().split("\n")
        held += sum(tmp.rstrip("/") in line for line in text)
        brief += sum(bool(BRIEF.search(line)) for line in text)
        holders += sum("<scratch" in line or "<tmp" in line for line in text)
    print(f"  redaction: {len(files)} capture files; lines holding the run's TMPDIR: {held}; lines the brief's pattern matches: {brief}; "
          f"lines holding a `<scratch` or `<tmp` placeholder: {holders}")
    for side in ("fx", "xfx"):
        hits = glob.glob(os.path.join(out, "runs", "workspace-index", side, f"{side}-workspace-index-*-at-plan.txt"))
        row = open(hits[0], encoding="utf-8").read().split("\n")[12].rstrip() if len(hits) == 1 else None
        print(f"  workspace-index at-plan row 13, {side}: {row!r} ({len(row) if row else 0} columns)")
if os.path.isdir(os.path.join(joins, "j2-1")):
    print()
    print("## join check 2: p5-input-row-limit five times, one after another (walk_rows.py)")
    sys.stdout.flush()
    subprocess.run([sys.executable, "-I", "-B", os.path.join(here, "walk_rows.py")]
                   + [os.path.join(joins, f"j2-{i}") for i in range(1, 6)], check=False)
