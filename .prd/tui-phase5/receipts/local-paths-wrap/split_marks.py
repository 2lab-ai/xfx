"""LOCAL-PATHS-WRAP rounds 4 and 5: whether any tracked file holds a split
mark, and what scan.sh's widened filter adds. Prints counts, never a row.

usage: git ls-files -z | python3 -I split_marks.py <scripts/fxcap>

1. **Split marks, by their bytes** (round 5, gpt6's MUST). A split mark is
   `┃` (e2 94 83), then one or more control sequences -- as roots.py's
   CONTROL reads one: CSI, OSC, nF -- then `↑` (e2 86 91), with no visible
   byte between. `↳` is one glyph and cannot be split. The count takes any
   control sequence, a cursor move among them, though one parts the rail
   from the arrow (two rows, no mark): for an absence, counting more is the
   safe side. It checks itself first: both split forms test_capture.py
   pins match; a plain `┃↑` and a `┃`, a reset, a space and `↑` do not.
2. **The widened filter** (round 4). scan.sh reads a file holding `↳`, or
   `┃` and `↑` anywhere; round 3's read one holding a mark's bytes in one
   piece. The rows behind a mark in each set, from one View per file: the
   widened filter adds no marked rows if the two agree. That says nothing
   about a split mark in a file that also holds an intact one (both sets
   read that file); part 1 does.
"""
import re
import sys

sys.path.insert(0, sys.argv[1])
sys.dont_write_bytecode = True
import roots  # noqa: E402

RAIL, ARROW = b"\xe2\x94\x83", b"\xe2\x86\x91"
SPLIT = re.compile(re.escape(RAIL) + b"(?:" + roots.CONTROL.pattern + b")+" + re.escape(ARROW))

for sample, split in ((b"\x1b[2m" + RAIL + b"\x1b[0m\x1b[1m" + ARROW + b"\x1b[0m someone/x", True),
                      (b"\x1b[2m" + RAIL + b"\x1b[0m\x1b[2m" + ARROW + b"\x1b[0m someone/x", True),
                      (RAIL + ARROW + b"someone/x", False),
                      (b"\x1b[2m" + RAIL + b"\x1b[0m " + ARROW + b"\xe2\x86\x93 move", False)):
    if bool(SPLIT.search(sample)) != split:
        sys.exit("split_marks.py: its own check failed on %r; nothing measured" % sample)
print("self-check: the 2 split forms match, the 2 that are not split do not")

rails = split_files = occurrences = 0
new_files = old_files = new_rows = old_rows = 0
for name in sys.stdin.buffer.read().split(b"\0"):
    try:
        data = open(name, "rb").read() if name else b""
    except OSError:
        continue
    if RAIL in data:
        rails += 1
        n = len(SPLIT.findall(data))
        split_files, occurrences = split_files + bool(n), occurrences + n
    if b"\xe2\x86\xb3" in data or (RAIL in data and ARROW in data):
        k = len(roots.View(data).marked)
        new_files, new_rows = new_files + 1, new_rows + k
        if any(m in data for m in roots.MARKS):
            old_files, old_rows = old_files + 1, old_rows + k
print("split marks, by their bytes: %d tracked files hold a rail, %d of them a split rail-arrow, %d occurrences"
      % (rails, split_files, occurrences))
print("the widened filter: %d files read, %d with round 3's filter; rows behind a mark in them %d and %d -- "
      "it adds %d marked rows" % (new_files, old_files, new_rows, old_rows, new_rows - old_rows))
