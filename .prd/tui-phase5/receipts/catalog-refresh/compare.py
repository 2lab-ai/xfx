#!/usr/bin/env python3
"""Every snap pair of two sweeps side by side: the base's verdict and the
head's, and the pairs whose verdict moved.

usage: python3 -I compare.py <base pairs.json> <head pairs.json>"""
import json, sys

def load(path):
    return {(p["scenario"], p["label"]): p["verdict"] for p in json.load(open(path))}

base, head = load(sys.argv[1]), load(sys.argv[2])
keys = sorted(set(base) | set(head))
good = ("identical", "normalized-identical")
print(f"pairs: base {len(base)}, head {len(head)}; passing: base {sum(v in good for v in base.values())}, head {sum(v in good for v in head.values())}")
moved = [k for k in keys if base.get(k) != head.get(k)]
print(f"moved: {len(moved)}")
for k in moved:
    print(f"  {k[0]}/{k[1]}: {base.get(k)} -> {head.get(k)}")
print()
for k in keys:
    print(f"{k[0]}/{k[1]}\t{base.get(k)}\t{head.get(k)}")
