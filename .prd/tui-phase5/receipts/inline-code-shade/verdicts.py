"""Two sweeps' pairs.json side by side: every pair's verdict before and after,
and the entries its differing rows matched. usage: python3 -I verdicts.py <before> <after>"""
import collections, json, sys
a = {(p["scenario"], p["label"]): p for p in json.load(open(sys.argv[1]))}
b = {(p["scenario"], p["label"]): p for p in json.load(open(sys.argv[2]))}
tb, ta = collections.Counter(), collections.Counter()
for k in sorted(a):
    pa, pb = a[k], b[k]
    tb[pa["verdict"]] += 1; ta[pb["verdict"]] += 1
    ids = sorted({i for r in pb.get("rows", []) for i in r.get("matched", [])})
    print("%-32s %-22s -> %-22s %s" % ("%s/%s" % k, pa["verdict"], pb["verdict"], ",".join(ids)))
print("# before:", dict(tb))
print("# after: ", dict(ta))
