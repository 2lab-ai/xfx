"""Every differing row of a sweep's pairs.json whose text holds the word `code`
on either side: scenario/snap, row, verdict of the pair, the entries that
matched the row, and whether it is a colour-only row (the same text on both
sides). usage: python3 -I coderows.py <pairs.json>"""
import collections, json, re, sys
pairs = json.load(open(sys.argv[1]))
pairs = pairs["pairs"] if isinstance(pairs, dict) else pairs
word = re.compile(r"\bcode\b")
tally = collections.Counter()
for p in pairs:
    for r in p.get("rows", []):
        fx, xfx = r.get("fx", ""), r.get("xfx", "")
        if not (word.search(fx) or word.search(xfx)):
            continue
        ids = r.get("matched") or []
        same = "colour-only" if fx == xfx else "text-differs"
        tally[(same, "inline-code-shade" in ids)] += 1
        print("%s/%s row %d [%s] pair=%s matched=%s" % (p["scenario"], p["label"], r["row"], same, p["verdict"], ",".join(ids) or "-"))
print("# rows with `code`:", dict((("%s, %s" % (k[0], "covered" if k[1] else "NOT covered")), v) for k, v in sorted(tally.items())))
