#!/usr/bin/env python3
"""Train 10: conflict blocks whose two sides differ only in reference numbers
(`path:N`, `:N-M`) -- both sides re-pointed the same references, each to its
own branch's lines. Each block must be equal on both sides once every number
after a `:` is masked; it is resolved as ours (`main`'s numbers), and the
numbers-only re-point commit after the last merge moves every reference to
the head's code (repoint.py), checked by repointed.py.

    python3 -I -B numbers_only.py <worktree-file>

Stops, changing nothing, if any block differs in more than numbers.
"""
import re
import sys

path = sys.argv[1]
NUMS = re.compile(r":\d+(?:-\d+)?")
lines = open(path, encoding="utf-8").read().split("\n")
out, i, blocks = [], 0, []
while i < len(lines):
    if not lines[i].startswith("<<<<<<< "):
        out.append(lines[i])
        i += 1
        continue
    mid = next(j for j in range(i, len(lines)) if lines[j] == "=======")
    end = next(j for j in range(mid, len(lines)) if lines[j].startswith(">>>>>>> "))
    o, t = lines[i + 1:mid], lines[mid + 1:end]
    if [NUMS.sub(":N", l) for l in o] != [NUMS.sub(":N", l) for l in t]:
        sys.exit(f"{path}:{i + 1}: the two sides differ in more than numbers")
    pairs = [(a, b) for x, y in zip(o, t) for a, b in zip(NUMS.findall(x), NUMS.findall(y)) if a != b]
    blocks.append((len(out) + 1, pairs))
    out.extend(o)
    i = end + 1
open(path, "w", encoding="utf-8").write("\n".join(out))
for at, pairs in blocks:
    print(f"{path}:{at}: ours' numbers kept; " + ", ".join(f"ours {a} / theirs {b}" for a, b in pairs))
print(f"# {len(blocks)} blocks, each the same on both sides but for its numbers")
