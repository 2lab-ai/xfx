#!/usr/bin/env python3
"""Train 13: probe.sh's two outputs lined up, one row per spelling: what
QUESTION-WORD-LEFT's head decodes it to (the question read it as it came),
what the train head's decoder makes of it, and what `unspelled` gives the
question there. Exit 0 only when the question reads every spelling as at
f686b736.

    python3 -I -B probe_table.py <probe-qwl.out> <probe-head.out>
"""
import sys

qwl, head = sys.argv[1:3]
q = [l[6:].rstrip("\n") for l in open(qwl) if l.startswith("PROBE ")]
fold = [l[6:].rstrip("\n") for l in open(head) if l.startswith("PROBE ")]
raw = [l[4:].rstrip("\n") for l in open(head) if l.startswith("RAW ")]
assert len(q) == len(fold) == len(raw), (len(q), len(fold), len(raw))
moved = [i for i in range(len(q)) if q[i] != raw[i]]
same = [i for i in range(len(q)) if q[i] == fold[i]]
print(f"# {len(q)} spellings; the head's decoder reads {len(moved)} of them otherwise than f686b736's "
      f"(the approval stack's reclassified spellings); the question reads {len(same)} of {len(q)} as at f686b736")
print("# the reclassified ones: spelling -> f686b736's decode | the head's decode | through unspelled")
for i in moved:
    spelling, then = q[i].split(" -> ", 1)
    print(f"{spelling} -> {then} | {raw[i].split(' -> ', 1)[1]} | {fold[i].split(' -> ', 1)[1]}"
          f"{'' if q[i] == fold[i] else '   DIFFERENT'}")
print("# the rest, the same on both decoders (spelling -> decode)")
for i in range(len(q)):
    if i not in moved:
        print(q[i] + ("" if q[i] == fold[i] else "   DIFFERENT"))
sys.exit(0 if len(same) == len(q) else 1)
