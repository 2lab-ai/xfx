#!/usr/bin/env python3
"""Resolves the one docs/parity.md conflict of the join with train 16.

    python3 -I resolve_parity.py <repo>     (run in the middle of the merge)

The conflict is the OSC 7501 row: this branch added one sentence to it, and
train 16 re-pointed two worker.rs references in it (3922 -> 3955,
3561 -> 3594). The resolution is train 16's row with this branch's sentence
put back after the same anchor, so both sides' text stands. It refuses
anything else: a second conflict, a row whose anchor is missing, or an
"ours" that differs from "theirs" by more than the sentence and those two
references. Prints no path.
"""
import os
import sys

path = os.path.join(sys.argv[1], "docs", "parity.md")
text = open(path).read()
OPEN, MID, CLOSE = "<<<<<<< HEAD\n", "=======\n", ">>>>>>> 8be4693b\n"
assert text.count(OPEN) == 1 and text.count(CLOSE) == 1, "not exactly one conflict"
a = text.index(OPEN)
m = text.index(MID, a)
b = text.index(CLOSE, m)
ours, theirs = text[a + len(OPEN):m], text[m + len(MID):b]
ANCHOR = ("xfx's band shows a prompt's `Thinking` once the runtime says the turn started, "
          "so a prompt's `working` waits for the tick whose frame first shows it")
MINE = (" -- and that tick reads the runtime no further than the start, so its frame shows "
        "`Thinking` however much of the turn had already been sent, and `working` is never "
        "skipped: upstream's keystroke holds its worker's start until the pass that has "
        "reported `working` (`finalizePendingSubmission`, `main.zig:1263-1274`; "
        "`input_submit_runtime.zig:391-405`).")
assert ours.count(ANCHOR + MINE) == 1, "this branch's sentence is not in ours"
assert theirs.count(ANCHOR + ".") == 1, "the anchor is not in theirs"
expected = ours.replace(ANCHOR + MINE, ANCHOR + ".")
expected = expected.replace("`worker.rs:3922`, then `:3561`", "`worker.rs:3955`, then `:3594`")
assert expected == theirs, "ours and theirs differ by more than the sentence and the two references"
resolved = theirs.replace(ANCHOR + ".", ANCHOR + MINE)
open(path, "w").write(text[:a] + resolved + text[b + len(CLOSE):])
print("resolved: train 16's OSC 7501 row, with this branch's sentence after its anchor")
