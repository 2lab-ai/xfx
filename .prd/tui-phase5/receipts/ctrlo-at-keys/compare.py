"""Base sweep against head sweep, pair by pair: every snap pair whose verdict
moved, and the count of each verdict on each side. A pair counts as passing
when it is identical or normalized-identical (sweep.py's pass). Then the
pairs that do not pass, each side, and whether they are the same set.
(AT-LAUNCH-WALK's, with that last listing written by the script itself.)

usage: python3 -I compare.py <base pairs.json> <head pairs.json>
"""

import collections
import json
import sys

PASS = ("identical", "normalized-identical")


def load(path):
    with open(path, encoding="utf-8") as f:
        return {(p["scenario"], p["label"]): p["verdict"] for p in json.load(f)}


def main():
    base, head = load(sys.argv[1]), load(sys.argv[2])
    keys = sorted(set(base) | set(head))
    moved = [(k, base.get(k), head.get(k)) for k in keys if base.get(k) != head.get(k)]
    up = [m for m in moved if m[2] in PASS and m[1] not in PASS]
    down = [m for m in moved if m[1] in PASS and m[2] not in PASS]
    print(f"pairs: base {len(base)}, head {len(head)}")
    for side, pairs in (("base", base), ("head", head)):
        counts = collections.Counter(pairs.values())
        passing = sum(counts[v] for v in PASS)
        print(f"{side}: pass {passing}/{len(pairs)}  " + ", ".join(f"{v} {n}" for v, n in sorted(counts.items())))
    print(f"\nmoved to passing: {len(up)}")
    for (s, l), b, h in up:
        print(f"  {s} {l}: {b} -> {h}")
    print(f"\ndropped from passing: {len(down)}")
    for (s, l), b, h in down:
        print(f"  {s} {l}: {b} -> {h}")
    other = [m for m in moved if m not in up and m not in down]
    print(f"\nother verdict changes: {len(other)}")
    for (s, l), b, h in other:
        print(f"  {s} {l}: {b} -> {h}")
    failing = {side: sorted((k, v) for k, v in pairs.items() if v not in PASS) for side, pairs in (("base", base), ("head", head))}
    rest = [row for row in failing["base"] if row[0] not in {k for k, _, _ in up}]
    print(f"\n# the pairs that do not pass, each side (scenario label verdict); the same set on both sides: {failing['base'] == failing['head']}; "
          f"the head's is the base's less those moved to passing: {rest == failing['head']}")
    for side, rows in failing.items():
        print(f"{side} ({len(rows)}): " + ", ".join(f"{s} {l} [{v}]" for (s, l), v in rows))


if __name__ == "__main__":
    main()
