"""Pair-by-pair verdicts of compaction sweeps, base against each head.
usage: python3 -I pairs_compare.py <base-dir> <head-dir>... ; a pair whose
verdict is a pass on the base and not on a head is a drop."""
import json, sys
PASS = {"identical", "normalized-identical"}
def load(d):
    return {(p["scenario"], p["label"]): p["verdict"] for p in json.load(open(d + "/pairs.json"))}
base = load(sys.argv[1]); heads = [load(d) for d in sys.argv[2:]]
drops = 0
print("scenario/label".ljust(48), "base".ljust(22), *[f"head{i+1}".ljust(22) for i in range(len(heads))])
for key in sorted(set(base) | set().union(*heads)):
    row = [base.get(key, "-")] + [h.get(key, "-") for h in heads]
    drop = row[0] in PASS and any(v not in PASS for v in row[1:])
    drops += drop
    print(f"{key[0]}/{key[1]}".ljust(48), *[v.ljust(22) for v in row], "DROP" if drop else "")
print(f"pairs: base {len(base)}, " + ", ".join(f"head{i+1} {len(h)}" for i, h in enumerate(heads)) + f"; drops {drops}")
