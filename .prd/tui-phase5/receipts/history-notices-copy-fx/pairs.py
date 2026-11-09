#!/usr/bin/env python3
"""The capture pairs of this folder, rows 2 on (row 1 is each binary's banner), as unified diffs.

    python3 -I -B pairs.py <dir with fx/, base-xfx/, xfx/>

For each scenario and snap: base-xfx vs xfx (what the fix changed), fx vs base-xfx (RED) and
fx vs xfx (GREEN), each `(identical)` or its diff, and the rows each side has that hold the
deleted line's words.
"""
import difflib
import os
import sys

SNAPS = {
    "provider-carry-back": ["start", "turn1", "open", "switched", "turn2", "turn3"],
    "provider-carry-back-stop": ["turn1", "stopped", "next"],
}
WORDS = ("recorded reasoning", "not carried over")


def rows(root, side, scenario, snap):
    name = f"{'fx' if side == 'fx' else 'xfx'}-{scenario}-40x120-{snap}.txt"
    with open(os.path.join(root, side, name), encoding="utf-8") as handle:
        return handle.read().split("\n")


def main():
    root = sys.argv[1]
    for side in ("fx", "base-xfx", "xfx"):
        print(f"# {side} row 1: {rows(root, side, 'provider-carry-back', 'start')[0]}")
    for scenario, snaps in SNAPS.items():
        for snap in snaps:
            got = {side: rows(root, side, scenario, snap) for side in ("fx", "base-xfx", "xfx")}
            for side, lines in got.items():
                held = [n + 1 for n, line in enumerate(lines) if any(word in line for word in WORDS)]
                print(f"## {scenario}/{snap} {side}: rows with the deleted line's words: {held or 'none'}")
            for a, b in (("base-xfx", "xfx"), ("fx", "base-xfx"), ("fx", "xfx")):
                diff = list(difflib.unified_diff(got[a][1:], got[b][1:], a, b, n=0, lineterm=""))
                print(f"### {scenario}/{snap}: {a} vs {b}, rows 2 on")
                print("\n".join(diff[2:]) if diff else "(identical)")


if __name__ == "__main__":
    main()
