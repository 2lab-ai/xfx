"""usage: compare_pairs.py <base-sweep-out> <head-sweep-out> <scenario-names-file> <verbs-tsv>
Every snap pair of the named scenarios, as base 690aa505's capture harness and
the head's classified it (sweep.py pairs.json), side by side: the verdict of
each pair, and whether its differing rows are the same rows."""
import collections
import json
import sys

base, head, names, verbs = sys.argv[1:5]
want = [l.strip() for l in open(names) if l.strip()]
uses = dict(l.rstrip("\n").split("\t") for l in open(verbs) if "\t" in l)


def load(d):
    return {(p["scenario"], p["label"]): p for p in json.load(open(d + "/pairs.json"))}


b, h = load(base), load(head)
same_verdict = same_rows = total = 0
changed = []
print("| Scenario | Changed verb(s) it uses | Snaps | Verdicts, base harness | Verdicts, head harness | Same verdict | Same rows |")
print("|---|---|---|---|---|---|---|")
for name in want:
    keys = sorted({k for k in b if k[0] == name} | {k for k in h if k[0] == name})
    vb = collections.Counter(b[k]["verdict"] if k in b else "missing" for k in keys)
    vh = collections.Counter(h[k]["verdict"] if k in h else "missing" for k in keys)
    sv = sum(1 for k in keys if k in b and k in h and b[k]["verdict"] == h[k]["verdict"])
    sr = sum(1 for k in keys if k in b and k in h and b[k].get("rows") == h[k].get("rows"))
    total += len(keys)
    same_verdict += sv
    same_rows += sr
    for k in keys:
        if not (k in b and k in h and b[k]["verdict"] == h[k]["verdict"]):
            changed.append((k, b.get(k, {}).get("verdict"), h.get(k, {}).get("verdict")))
    fmt = lambda c: ", ".join("%s %d" % (v, n) for v, n in sorted(c.items()))
    print("| `%s` | %s | %d | %s | %s | %d/%d | %d/%d |" % (name, uses.get(name, "?"), len(keys), fmt(vb), fmt(vh), sv, len(keys), sr, len(keys)))
print()
print("pairs %d; same verdict %d; same differing rows %d" % (total, same_verdict, same_rows))
for k, x, y in changed:
    print("verdict changed: %s/%s base=%s head=%s" % (k[0], k[1], x, y))
