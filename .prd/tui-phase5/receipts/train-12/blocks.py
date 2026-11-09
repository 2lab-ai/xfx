#!/usr/bin/env python3
"""Train 11: a file's conflict blocks, written in git's diff3 style (ours,
`|||||||` base, `=======` theirs), each resolved by the one rule that fits
it, or none -- then it stops, changing nothing, to be read by hand:

  numbers  ours and theirs the same once every `:N` / `:N-M` is masked (both
           re-pointed one reference, each to its own branch's lines): ours,
           and the re-point commit after the last merge moves it to the head's
           code (train 10's numbers_only.py rule);
  rewrite  ours the same as the base once its numbers are masked, theirs not:
           theirs (ours moved only numbers in text theirs rewrote; whatever
           theirs cites is read by the re-point scans after the last merge);
  append   (only with --append) the base empty, both sides inserted lines
           there: ours' lines, then theirs' (train 9's and 10's rule).

    python3 -I -B blocks.py [--append] <worktree-file>
"""
import re
import sys

args = sys.argv[1:]
append = "--append" in args
path = [a for a in args if a != "--append"][0]
NUMS = re.compile(r":\d+(?:-\d+)?")


def masked(lines):
    return [NUMS.sub(":N", l) for l in lines]


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
    if masked(o) == masked(t):
        pairs = [(x, y) for p, q in zip(o, t) for x, y in zip(NUMS.findall(p), NUMS.findall(q)) if x != y]
        said.append(f"{path}:{at}: numbers -- ours kept; " + ", ".join(f"ours {x} / theirs {y}" for x, y in pairs))
        out.extend(o)
    elif masked(o) == masked(b):
        pairs = [(x, y) for p, q in zip(b, o) for x, y in zip(NUMS.findall(p), NUMS.findall(q)) if x != y]
        said.append(f"{path}:{at}: rewrite -- theirs taken ({len(b)} base lines -> {len(t)}); ours had moved only " + ", ".join(f"{x} -> {y}" for x, y in pairs))
        out.extend(t)
    elif append and not b and o and t:
        said.append(f"{path}:{at}: append -- both inserted at one base place: ours' {len(o)} lines, then theirs' {len(t)}")
        out.extend(o + t)
    else:
        sys.exit(f"{path}:{i + 1}: no rule fits this block -- read by hand")
    i = end + 1
open(path, "w", encoding="utf-8").write("\n".join(out))
print("\n".join(said))
print(f"# {path}: {len(said)} blocks resolved")
