"""Base sweep against head sweep, pair by pair (pairs.json of each).
TG-REDRAW's and T-U4's (`../cancel-marker/cmp_sweeps.py`), with the
per-side list of uncaused rows cut to a count: those rows are screen text,
and this output is committed.

usage: python3 -I cmp_sweeps.py <base pairs.json> <head pairs.json>

A pair "drops" when it passed (identical / normalized-identical) on the base
and does not on the head, or when its class gets worse. Prints every pair whose
class or cause list changed, the pairs only one side has, and a count.
"""
import json, sys
RANK = {"identical": 0, "normalized-identical": 0, "approved-divergence": 1,
        "approved-divergence-cascade": 1, "replaced-in-next-pin": 1,
        "unresolved-defect": 2, "failed": 3, "timeout": 3}
def load(p):
    return {(x["scenario"], x["label"]): x for x in json.load(open(p))}
base, head = load(sys.argv[1]), load(sys.argv[2])
drops = 0
for key in sorted(set(base) | set(head)):
    b, h = base.get(key), head.get(key)
    if b is None or h is None:
        print("only-%s %s %s" % ("head" if b is None else "base", key, (h or b)["verdict"]))
        continue
    bc, hc = b["verdict"], h["verdict"]
    bcause = sorted(b.get("causes") or [])
    hcause = sorted(h.get("causes") or [])
    if bc != hc or bcause != hcause or len(b.get("unattributed") or []) != len(h.get("unattributed") or []):
        worse = RANK[hc] > RANK[bc]
        drops += worse
        print("%s %s/%s: %s -> %s; causes %s -> %s; uncaused %d -> %d" % (
            "DROP" if worse else "change", key[0], key[1], bc, hc, bcause, hcause,
            len(b.get("unattributed") or []), len(h.get("unattributed") or [])))
for name, side in (("base", base), ("head", head)):
    unc = sum(1 for v in side.values() if v.get("unattributed"))
    print("%s: %d pairs, %d pass, %d with uncaused rows" % (
        name, len(side), sum(RANK[v["verdict"]] == 0 for v in side.values()), unc))
print("drops: %d" % drops)
sys.exit(1 if drops else 0)
