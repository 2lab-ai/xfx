"""LOCAL-PATHS-WRAP round 2 (opus Q2.4): the committed files in which a row
ends in `<local-path>` (redact.py's token for a path it took out) and the
next row goes on, behind spaces or a box rail and no mark, with a path piece
(`A-Za-z0-9._~+-` then a `/`). roots.py's tail rule reads behind a mark only,
so such a row passes it. Prints counts, never a row.

usage: git ls-files -z .prd | python3 -I gutter_tails.py
"""
import re
import sys

ROW = re.compile(rb"<local-path>[ ]*\n(?:[ ]|\xe2\x94\x82|\xe2\x94\x83)*[A-Za-z0-9._~+-]*/[A-Za-z0-9._~+/-]+")
files = rows = 0
for name in sys.stdin.buffer.read().split(b"\0"):
    if not name:
        continue
    try:
        data = open(name, "rb").read()
    except OSError:
        continue
    n = len(ROW.findall(data))
    files, rows = files + bool(n), rows + n
print("tracked files under .prd/ with a path piece behind a plain gutter after <local-path>: %d (%d rows)" % (files, rows))
