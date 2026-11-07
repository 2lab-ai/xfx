"""Two sweeps of the same scenarios, pair by pair: what the head changed
against the base.

usage: python3 -I -B compare.py <base-sweep-out> <head-sweep-out>

Reads each sweep's `pairs.json` and its captures (`runs/<scenario>/<side>/`).
Prints the counts per verdict on each side; every pair whose verdict moved;
every pair whose differing rows (fx text, xfx text) are not the same set on
both; and, per side, every snap whose screen (`.ansi`, row 1 -- the banner --
left out) is not the same in the two sweeps. fx is one binary in both, so an
fx screen that moved is the run's own timing; an xfx screen that moved is the
head's change or its timing.

Exit 0 when every pair that passes on the base passes on the head ("pass" is
identical or normalized-identical, the sweep's own measure) **or** moved only
because fx's own screen did while xfx's stayed the same.
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


base, head = (load(p) for p in sys.argv[1:3])
assert set(base) == set(head), "the two sweeps took different pairs"
for name, side in (("base", base), ("head", head)):
    counts = collections.Counter(p["verdict"] for p in side.values())
    passed = sum(counts[v] for v in PASS)
    print("%s: %d pairs, pass %d; %s" % (name, len(side), passed, ", ".join("%s %d" % kv for kv in sorted(counts.items()))))
dropped, gained, moved = [], [], []
for key in sorted(base):
    b, h = base[key], head[key]
    rows = lambda p: {(r["fx"], r["xfx"]) for r in p["rows"]}  # noqa: E731
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
for key in sorted(base):
    for side in ("fx", "xfx"):
        a, b = screen(sys.argv[1], key[0], side, key[1]), screen(sys.argv[2], key[0], side, key[1])
        if a is None or b is None or a != b:
            changed[side].add(key)
for side in ("fx", "xfx"):
    print("%s screens not the same in the two sweeps: %d%s" % (side, len(changed[side]), "".join(
        "\n    %s %s" % key for key in sorted(changed[side]))))
excused = [key for key in dropped if key in changed["fx"] and key not in changed["xfx"]]
print("passed on the base, not on the head: %d; of them fx's screen moved and xfx's did not: %d" % (len(dropped), len(excused)))
print("pairs that gained a differing row: %d" % len(gained))
sys.exit(1 if set(dropped) - set(excused) else 0)
