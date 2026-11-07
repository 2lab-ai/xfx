"""Base sweeps against head sweeps, pair by pair, every run of each.

usage: python3 -I pairs_compare.py --base <pairs.json>... --head <pairs.json>...

A pair is a scenario's snap label. For each pair it prints the verdict of
every base run and every head run when they are not all the same, and DROP
for a pair every base run passed (identical or normalized-identical) that a
head run does not. Exit 1 on a DROP.
"""

import json
import sys

PASS = ("identical", "normalized-identical")


def runs(paths):
    out = {}
    for path in paths:
        with open(path) as f:
            for p in json.load(f):
                out.setdefault((p["scenario"], p["label"]), []).append(p["verdict"])
    return out


def main(argv):
    i = argv.index("--head")
    base, head = runs(argv[2:i]), runs(argv[i + 1 :])
    keys = sorted(set(base) | set(head))
    print("pairs: base %d, head %d; runs: base %d, head %d" % (len(base), len(head), len(argv[2:i]), len(argv[i + 1 :])))
    for side, d in (("base", base), ("head", head)):
        every = sum(len(v) for v in d.values())
        print("%s: %d of %d pair runs pass; pairs passing in every run: %d of %d" % (
            side, sum(1 for v in d.values() for x in v if x in PASS), every,
            sum(1 for v in d.values() if all(x in PASS for x in v)), len(d)))
    drops, changed = 0, 0
    for k in keys:
        a, b = base.get(k, []), head.get(k, [])
        if len(set(a + b)) <= 1 and a and b:
            continue
        changed += 1
        drop = a and all(x in PASS for x in a) and any(x not in PASS for x in b)
        drops += bool(drop)
        print("%s %s/%s: base %s -> head %s" % ("DROP" if drop else "    ", k[0], k[1], ",".join(a) or "(absent)", ",".join(b) or "(absent)"))
    print("pairs whose verdicts are not one and the same across all runs: %d; drops: %d" % (changed, drops))
    return 1 if drops else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
