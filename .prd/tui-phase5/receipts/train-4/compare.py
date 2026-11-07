#!/usr/bin/env python3
"""Two sweeps, pair by pair, over the pairs both took: what the train head
changed against a reference sweep (main's, or a unit's own head sweep).

    python3 -I -B compare.py <reference-sweep-out> <train-sweep-out>

ENGINE-VS-ANCHOR's `compare.py`
(`.prd/tui-phase5/receipts/engine-vs-anchor/compare.py`), as train 3 had it,
with one more change for train 4: a pair that failed or timed out has no
`rows` and reads as none. Train 3's change:
the two sweeps need not take the same pairs (a unit's sweep took a subset of
the train's), so it reads the pairs both took and counts the rest.

Prints the counts per verdict on each side over the shared pairs; every pair
whose verdict moved; every pair whose differing rows (fx text, xfx text) are
not the same set on both; and, per side, every snap whose screen (`.ansi`,
row 1 -- the banner -- left out) is not the same in the two sweeps. fx is one
binary in both, so an fx screen that moved is the run's own timing; an xfx
screen that moved is the code's change or its timing.

Exit 0 when every shared pair that passes on the reference passes on the
train ("pass" is identical or normalized-identical, the sweep's own measure)
**or** moved only because fx's own screen did while xfx's stayed the same.
"""
import collections
import glob
import json
import os
import re
import sys

PASS = ("identical", "normalized-identical")


def load(out):
    with open(os.path.join(out, "pairs.json"), encoding="utf-8") as f:
        return {(p["scenario"], p["label"]): p for p in json.load(f)}


base_all, head_all = (load(p) for p in sys.argv[1:3])
shared = sorted(set(base_all) & set(head_all))
print("pairs: reference %d, train %d, shared %d; only the reference: %d%s" % (
    len(base_all), len(head_all), len(shared), len(set(base_all) - set(head_all)),
    "".join("\n    %s %s" % k for k in sorted(set(base_all) - set(head_all)))))
base = {k: base_all[k] for k in shared}
head = {k: head_all[k] for k in shared}
for name, side in (("reference", base), ("train", head)):
    counts = collections.Counter(p["verdict"] for p in side.values())
    passed = sum(counts[v] for v in PASS)
    print("%s: %d pairs, pass %d; %s" % (name, len(side), passed, ", ".join("%s %d" % kv for kv in sorted(counts.items()))))
dropped, gained, moved = [], [], []
for key in shared:
    b, h = base[key], head[key]
    rows = lambda p: {(r["fx"], r["xfx"]) for r in p.get("rows", [])}  # noqa: E731  (a timed-out pair has none)
    if b["verdict"] != h["verdict"]:
        moved.append("%s %s: %s -> %s" % (key[0], key[1], b["verdict"], h["verdict"]))
        if b["verdict"] in PASS and h["verdict"] not in PASS:
            dropped.append(key)
    new = rows(h) - rows(b)
    gone = rows(b) - rows(h)
    if new or gone:
        print("%s %s: %d differing rows left, %d new" % (key[0], key[1], len(gone), len(new)))
        for fx, xfx in sorted(gone):
            print("    left: fx %r / xfx %r" % (fx[-70:], xfx[-70:]))
        for fx, xfx in sorted(new):
            print("    NEW:  fx %r / xfx %r" % (fx[-70:], xfx[-70:]))
        if new:
            gained.append(key)
print("verdicts moved: %d%s" % (len(moved), "".join("\n    " + m for m in moved)))


def screen(out, scenario, side, label):
    prefix = "fx" if side == "fx" else "xfx"
    exact = re.compile(r"^%s-%s-\d+x\d+-%s\.ansi$" % (prefix, re.escape(scenario), re.escape(label)))
    found = [p for p in glob.glob(os.path.join(out, "runs", scenario, side, "*.ansi")) if exact.match(os.path.basename(p))]
    if len(found) != 1:
        return None
    with open(found[0], encoding="utf-8") as f:
        return f.read().split("\n")[1:]


changed = {"fx": set(), "xfx": set()}
for key in shared:
    for side in ("fx", "xfx"):
        a, b = screen(sys.argv[1], key[0], side, key[1]), screen(sys.argv[2], key[0], side, key[1])
        if a is None or b is None or a != b:
            changed[side].add(key)
for side in ("fx", "xfx"):
    print("%s screens not the same in the two sweeps: %d%s" % (side, len(changed[side]), "".join(
        "\n    %s %s" % key for key in sorted(changed[side]))))
excused = [key for key in dropped if key in changed["fx"] and key not in changed["xfx"]]
print("passed on the reference, not on the train: %d%s; of them fx's screen moved and xfx's did not: %d" % (
    len(dropped), "".join("\n    %s %s" % k for k in dropped), len(excused)))
print("pairs that gained a differing row: %d" % len(gained))
sys.exit(1 if set(dropped) - set(excused) else 0)
