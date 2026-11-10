"""Why each pair of a scenario (default p5-mcp-verbs) is unresolved: per pair,
how many rows are left without a match, how many of them hold a server's
`connecting`/`retrying`, and the first one (pairs.json, as the sweep scored
it).

usage: python3 -I verbs_rows.py [--scenario <name>] <sweep out>...

The first row is printed as it is, spaces kept: a column one cell wider
shows there.
"""
import json
import os
import re
import sys

WORD = re.compile(r"\b(connecting|retrying)\b")
args = sys.argv[1:]
scen = "p5-mcp-verbs"
if args[:1] == ["--scenario"]:
    scen, args = args[1], args[2:]
for out in args:
    print("== %s" % os.path.basename(out.rstrip("/")))
    for p in json.load(open(os.path.join(out, "pairs.json"))):
        if p["scenario"] != scen:
            continue
        rows = [r for r in p["rows"] if not r.get("matched")]
        word = [r for r in rows if WORD.search((r.get("fx") or "") + " " + (r.get("xfx") or ""))]
        first = rows[0] if rows else {}
        other = len(rows) - len(word)
        print("%-15s %-18s unmatched %2d: with the word %d, without %2d; first row %s:"
              % (p["label"], p["verdict"], len(rows), len(word), other, first.get("row", "-")))
        print("    fx  %r" % (first.get("fx") or "").rstrip()[:70])
        print("    xfx %r" % (first.get("xfx") or "").rstrip()[:70])
