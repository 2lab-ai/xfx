#!/usr/bin/env python3
"""Two sweeps' pairs, row by row: for every pair, whether the rows no ledger entry covers
(`unattributed`) and the causes named are the same on the head as on the base.

    python3 -I -B pair_rows.py <base pairs.json> <head pairs.json> [<scenario>]

Prints every pair whose unattributed rows or causes differ, then, for <scenario>, each snap's
verdicts, causes and unattributed rows.
"""
import json
import sys


def load(path):
    with open(path, encoding="utf-8") as handle:
        return {(pair["scenario"], pair["label"]): pair for pair in json.load(handle)}


def rows(pair):
    return [(side, text) for side, _, text in pair.get("unattributed", [])]


def causes(pair):
    return [cause if isinstance(cause, str) else cause.get("id", str(cause)) for cause in pair.get("causes", [])]


def main():
    base, head = load(sys.argv[1]), load(sys.argv[2])
    moved = 0
    for key in sorted(set(base) & set(head)):
        if rows(base[key]) != rows(head[key]) or causes(base[key]) != causes(head[key]):
            moved += 1
            print(f"{key[0]}/{key[1]}: unattributed {len(rows(base[key]))} -> {len(rows(head[key]))}, "
                  f"same rows: {rows(base[key]) == rows(head[key])}; causes {causes(base[key])} -> {causes(head[key])}")
    print(f"pairs compared: {len(set(base) & set(head))}; pairs whose rows or causes moved: {moved}")
    if len(sys.argv) > 3:
        for key in sorted(k for k in head if k[0] == sys.argv[3]):
            print(f"## {key[1]}: base {base[key]['verdict']} -> head {head[key]['verdict']}; causes {causes(base[key])} -> {causes(head[key])}")
            for side, text in rows(head[key]):
                print(f"    {side}: {text}")


if __name__ == "__main__":
    main()
