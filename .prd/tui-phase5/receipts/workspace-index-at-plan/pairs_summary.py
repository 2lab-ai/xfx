import json, os, sys
# usage: pairs_summary.py <sweep out dir> -- each pair's verdict and its first
# differing row; for at-plan, the row on which each side lists `.../extra/notes/`
# (the folder `@plan` reaches only through the whole path), or `-` for none.
out = sys.argv[1]
ps = json.load(open(os.path.join(out, "pairs.json")))


def notes_row(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        rows = f.read().split("\n")
    hits = [i + 1 for i, r in enumerate(rows) if r.rstrip().endswith("/extra/notes/") and "@" not in r]
    return str(hits[0]) if hits else "-"


counts = {}
for p in sorted(ps, key=lambda p: (p["scenario"], p["label"])):
    counts[p["verdict"]] = counts.get(p["verdict"], 0) + 1
    extra = ""
    if p["label"] == "at-plan":
        d = os.path.join(out, "runs", p["scenario"])
        f = notes_row(os.path.join(d, "fx", "fx-%s-40x120-at-plan.txt" % p["scenario"]))
        x = notes_row(os.path.join(d, "xfx", "xfx-%s-40x120-at-plan.txt" % p["scenario"]))
        extra = "notes/ row: fx %s, xfx %s" % (f, x)
    rows = p.get("rows") or []
    first = ""
    if rows:
        r = rows[0]
        # Same text on both sides: the difference is in the cells' attributes (the bold letters).
        first = "first row %d: %s" % (r["row"], "same text, attributes differ" if r.get("fx") == r.get("xfx") else "text differs")
    print("%-24s %-8s %-21s %-26s %s" % (p["scenario"], p["label"], p["verdict"], extra, first))
print("totals:", json.dumps(counts, sort_keys=True))
