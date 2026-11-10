#!/usr/bin/env python3
"""The join of main into train 13 (train 13's blocks.py, its `numbers` rule
read token by token): a file's conflict blocks, written in git's diff3 style
(ours, `|||||||` base, `=======` theirs). Every block of this join is one of
references whose text the three sides share once every `:N` / `:N-M` is
masked; each `:N` token is then merged on its own:

  theirs  ours holds the base's number, theirs moved it: theirs' number
  ours    theirs holds the base's number, ours moved it: ours' number
  both    both moved it, each by its own branch's lines: ours' number, and
          the re-point commit after the merge moves it to the merged code
          (train 13's `numbers` rule)

A block whose three sides are not the same once masked stops the script,
changing nothing, to be read by hand. Prints one line per block: where, then
each token as base / ours / theirs -> merged and the rule.

    python3 -I -B blocks.py <worktree-file> [<label>]
"""
import re
import sys

path = sys.argv[1]
label = sys.argv[2] if len(sys.argv) > 2 else path
NUMS = re.compile(r":\d+(?:-\d+)?")


def masked(lines):
    return [NUMS.sub(":N", x) for x in lines]


lines = open(path, encoding="utf-8").read().split("\n")
out, i, said = [], 0, []
while i < len(lines):
    if not lines[i].startswith("<<<<<<< "):
        out.append(lines[i])
        i += 1
        continue
    bar = next(j for j in range(i, len(lines)) if lines[j].startswith("||||||| "))
    mid = next(j for j in range(bar, len(lines)) if lines[j] == "=======")
    end = next(j for j in range(mid, len(lines)) if lines[j].startswith(">>>>>>> "))
    o, b, t = lines[i + 1:bar], lines[bar + 1:mid], lines[mid + 1:end]
    at = len(out) + 1
    if not (masked(o) == masked(b) == masked(t)):
        sys.exit(f"{label}:{i + 1}: the three sides differ beyond their numbers -- read by hand")
    merged, notes = [], []
    for k, (lo, lb, lt) in enumerate(zip(o, b, t)):
        no, nb, nt = NUMS.findall(lo), NUMS.findall(lb), NUMS.findall(lt)
        pick = []
        for x, y, z in zip(no, nb, nt):
            if x == y == z:
                pick.append(x)
                continue
            rule, m = ("theirs", z) if x == y else ("ours", x) if z == y else ("both", x)
            pick.append(m)
            notes.append(f"line {at + k}: base {y} / ours {x} / theirs {z} -> {m} ({rule})")
        parts = NUMS.split(lo)
        merged.append(parts[0] + "".join(p + s for p, s in zip(pick, parts[1:])))
    out.extend(merged)
    said.append(f"{label}:{at}: {len(o)} line(s); " + "; ".join(notes))
    i = end + 1
open(path, "w", encoding="utf-8").write("\n".join(out))
print("\n".join(said))
print(f"# {label}: {len(said)} blocks resolved")
