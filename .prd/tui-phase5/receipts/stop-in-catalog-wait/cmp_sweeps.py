#!/usr/bin/env python3
"""Two sweeps' pairs, side by side: every pair that passes (identical or
normalized-identical) on the base must pass on the head.

    python3 -I -B cmp_sweeps.py <base pairs.json> <head pairs.json>

Prints each pair whose verdict differs, the counts, and `dropped: N`; exit 1
when a pair the base passed does not pass on the head, or a pair is missing.
"""
import json
import sys

PASS = ("identical", "normalized-identical")


def load(path):
    with open(path, encoding="utf-8") as handle:
        return {(pair["scenario"], pair["label"]): pair["verdict"] for pair in json.load(handle)}


def main():
    base, head = load(sys.argv[1]), load(sys.argv[2])
    dropped = 0
    for key in sorted(set(base) | set(head)):
        was, now = base.get(key, "missing"), head.get(key, "missing")
        if was != now:
            print(f"{key[0]}/{key[1]}: base {was} -> head {now}")
        if (was in PASS and now not in PASS) or now == "missing":
            dropped += 1
    passes = lambda verdicts: sum(verdict in PASS for verdict in verdicts.values())
    print(f"base: {passes(base)} / {len(base)} pass; head: {passes(head)} / {len(head)} pass; dropped: {dropped}")
    return 1 if dropped else 0


if __name__ == "__main__":
    sys.exit(main())
