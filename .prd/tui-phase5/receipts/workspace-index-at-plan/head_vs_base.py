import json, os, sys
# usage: head_vs_base.py <head sweep out> <base sweep out> -- every snap pair's
# verdict on each build, pair by pair, and the pairs whose verdict differs.
PASS = ("identical", "normalized-identical")


def pairs(out):
    with open(os.path.join(out, "pairs.json")) as f:
        return {(p["scenario"], p["label"]): p["verdict"] for p in json.load(f)}


head, base = pairs(sys.argv[1]), pairs(sys.argv[2])
moved = []
for k in sorted(set(head) | set(base)):
    h, b = head.get(k, "-"), base.get(k, "-")
    mark = ""
    if (h in PASS) != (b in PASS):
        mark = "  <-- " + ("regression" if b in PASS else "fixed")
        moved.append(k)
    print("%-28s %-16s head %-28s base %s%s" % (k[0], k[1], h, b, mark))
n_pass = lambda d: sum(v in PASS for v in d.values())
print("pairs: head %d (%d pass), base %d (%d pass); verdicts that cross pass/fail: %d %s"
      % (len(head), n_pass(head), len(base), n_pass(base), len(moved), ["%s/%s" % k for k in moved]))
