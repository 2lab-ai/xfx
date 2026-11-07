"""Pair by pair, two sweeps' verdicts under one ledger: which pairs pass
(identical, normalized-identical) on one build and not on the other.
usage: python3 -I flips.py <base pairs.json> <head pairs.json>"""
import json, sys
PASS = {"identical", "normalized-identical"}
def load(p):
    return {(x["scenario"], x["label"]): x["verdict"] for x in json.load(open(p))}
b, h = load(sys.argv[1]), load(sys.argv[2])
keys = sorted(set(b) | set(h))
up = [k for k in keys if k in b and k in h and b[k] not in PASS and h[k] in PASS]
down = [k for k in keys if k in b and k in h and b[k] in PASS and h[k] not in PASS]
other = [k for k in keys if k in b and k in h and (b[k] in PASS) == (h[k] in PASS) and b[k] != h[k]]
print(f"pairs: base {len(b)}, head {len(h)}, both {len(set(b) & set(h))}")
print(f"base fails, head passes ({len(up)}):")
for k in up: print(f"  {k[0]}/{k[1]}: {b[k]} -> {h[k]}")
print(f"base passes, head fails ({len(down)}):")
for k in down: print(f"  {k[0]}/{k[1]}: {b[k]} -> {h[k]}")
print(f"same side, another verdict ({len(other)}):")
for k in other: print(f"  {k[0]}/{k[1]}: {b[k]} -> {h[k]}")
