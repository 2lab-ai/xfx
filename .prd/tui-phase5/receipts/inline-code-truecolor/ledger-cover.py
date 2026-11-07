"""Every cause that covers each unmatched row of each unresolved pair (not
only the first, which is the row's): `scenario snap side row: ids | text`."""
import json
import os
import sys

sys.dont_write_bytecode = True
fxcap, repo, ledger, out = sys.argv[1:5]
sys.path.insert(0, fxcap)
import ledger as ledgermod  # noqa: E402

led = ledgermod.load(ledger, repo)
for p in json.load(open(os.path.join(out, "pairs.json"), encoding="utf-8")):
    for a in p.get("aligned", []):
        if a["matched"] or not a["text"].strip():
            continue
        ids = [c["id"] for c in ledgermod.covering(led["causes"], p["scenario"], p["label"], a["row"], a["side"], a["text"])]
        print("%-30s %-12s %-4s %2d: %-48s | %s" % (p["scenario"], p["label"], a["side"], a["row"], ",".join(ids) or "(none)", a["text"]))
