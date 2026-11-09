"""Every snap pair of two sweeps, verdict against verdict, as a TSV and a
summary line."""
import collections, json, sys
a = {(p["scenario"], p["label"]): p["verdict"] for p in json.load(open(sys.argv[1] + "/pairs.json"))}
b = {(p["scenario"], p["label"]): p["verdict"] for p in json.load(open(sys.argv[2] + "/pairs.json"))}
out = open(sys.argv[3], "w")
out.write("scenario\tlabel\tbefore\tafter\n")
changed = []
for k in sorted(set(a) | set(b)):
    va, vb = a.get(k, "(absent)"), b.get(k, "(absent)")
    out.write("%s\t%s\t%s\t%s\n" % (k[0], k[1], va, vb))
    if va != vb:
        changed.append((k, va, vb))
print("pairs: before %d, after %d; same verdict %d, changed %d" % (len(a), len(b), sum(1 for k in a if b.get(k) == a[k]), len(changed)))
print("before:", dict(collections.Counter(a.values())))
print("after: ", dict(collections.Counter(b.values())))
for k, va, vb in changed:
    print("CHANGED %s/%s: %s -> %s" % (k[0], k[1], va, vb))
