"""ledger.txt's body from a sweep's --out: the totals and cause tables of
summary.md (the cause's `what` elided), whether any row has no cause, then
one line per pair: scenario, snap, verdict, its causes, (row, xfx text) of
the rows that differ."""
import json
import os
import sys

out = sys.argv[1]
summary = open(os.path.join(out, "summary.md"), encoding="utf-8").read().split("\n")
board = json.load(open(os.path.join(out, "scoreboard.json"), encoding="utf-8"))
pairs = json.load(open(os.path.join(out, "pairs.json"), encoding="utf-8"))


def table(head):
    i = next(n for n, line in enumerate(summary) if line.startswith(head))
    rows = []
    while i < len(summary) and summary[i].startswith("|"):
        rows.append(summary[i])
        i += 1
    return rows


print("\n".join(table("| total | ran |")))
print()
for n, line in enumerate(table("| cause | kind |")):
    cols = line.split(" | ")
    if n >= 2:
        cols[4] = "…"
    print(" | ".join(cols))
print()
un = board["causes"]["unattributed"]
print("(no row without a cause)" if not un else "rows without a cause: %s" % json.dumps(un, ensure_ascii=False))
print()
for p in pairs:
    rows = [(r["row"], r["xfx"]) for r in p.get("rows", [])]
    causes = ",".join(p.get("causes", [])) or "-"
    print("%-32s %-12s %-21s %-50s %s" % (p["scenario"], p["label"], p["verdict"], causes, rows))
