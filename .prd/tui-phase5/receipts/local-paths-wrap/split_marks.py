"""LOCAL-PATHS-WRAP round 4 (opus SHOULD 1): how many tracked files scan.sh's
mark count reads now -- a file with `↳`, or with `┃` and `↑` anywhere -- and
how many round 3's filter read (a mark's bytes in one piece), and the rows
behind a mark in each set: a split mark (`┃`, a style, `↑`) in a committed
file would show as rows only the new set has. Prints counts, never a row.

usage: git ls-files -z | python3 -I split_marks.py <scripts/fxcap>
"""
import sys

sys.path.insert(0, sys.argv[1])
sys.dont_write_bytecode = True
import roots  # noqa: E402

new_files = old_files = new_rows = old_rows = 0
for name in sys.stdin.buffer.read().split(b"\0"):
    try:
        data = open(name, "rb").read() if name else b""
    except OSError:
        continue
    new = b"\xe2\x86\xb3" in data or (b"\xe2\x94\x83" in data and b"\xe2\x86\x91" in data)
    old = any(m in data for m in roots.MARKS)
    if not new:
        continue
    k = len(roots.View(data).marked)
    new_files, new_rows = new_files + 1, new_rows + k
    if old:
        old_files, old_rows = old_files + 1, old_rows + k
print("files read: %d now, %d with round 3's filter; rows behind a mark in them: %d now, %d with round 3's filter; "
      "rows only a split mark could add: %d" % (new_files, old_files, new_rows, old_rows, new_rows - old_rows))
