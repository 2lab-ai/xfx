"""A path xfx wraps behind its `↳` mark, in a published capture.

usage: python3 -I gutter_check.py <capture .txt> <root> <repo>

Prints, without the path: each row that starts with `↳` and holds a piece of
<root> (its first or last bytes), how many bytes of the root it holds, and
what roots.py --check says of the file with that root given.
"""
import subprocess
import sys

cap, root, repo = sys.argv[1:4]
rows = open(cap, encoding="utf-8").read().split("\n")
for i, row in enumerate(rows, 1):
    s = row.strip()
    if not s.startswith("↳"):
        continue
    piece = s[1:].strip()
    head = max((n for n in range(len(piece) + 1) if root.startswith(piece[:n])), default=0)
    tail = max((n for n in range(len(root) + 1) if n and piece.startswith(root[-n:])), default=0)
    held = max(head, tail)
    where = "the root's first" if head >= tail else "the root's last"
    print("row %d starts with U+21B3 and holds %s %d of its %d bytes" % (i, where, held, len(root.encode())))
r = subprocess.run(["python3", "-I", repo + "/scripts/fxcap/roots.py", "--root", root, "scenarios", "--check", "--", cap],
                   capture_output=True, text=True)
print("roots.py --check exit %d" % r.returncode)
