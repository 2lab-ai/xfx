#!/usr/bin/env python3
"""Train 14: train 13's blocks.py (its rules `numbers`, `rewrite`, `ours`,
`append`), with its `numbers` rule read token by token as train 13's join of
main read it (`../train-13/join-main/blocks.py`). A file's conflict blocks,
written in git's diff3 style (ours, `|||||||` base, `=======` theirs), each
resolved by the one rule that fits it, or none -- then it stops, changing
nothing, to be read by hand:

  numbers  ours, base and theirs the same once every `:N` / `:N-M` is masked
           (references re-pointed, each side to its own branch's lines); each
           `:N` token merged on its own -- the side that moved it wins, and a
           number both moved keeps ours', which the re-point after the last
           merge moves to the merged code. Ours and theirs the same once
           masked but the base not (both rewrote it alike): the same, read
           against ours.
  rewrite  ours the same as the base once its numbers are masked, theirs not:
           theirs (ours moved only numbers in text theirs rewrote).
  ours     theirs the same as the base once its numbers are masked, ours not:
           ours (theirs moved only numbers in text ours rewrote).
  append   (only with --append) the base empty, both sides inserted lines
           there: ours' lines, then theirs'.

Prints one line per block: where, the rule, and for `numbers` each token
moved as base / ours / theirs -> merged.

    python3 -I -B blocks.py [--append] <worktree-file> [<label>]
"""
import re
import sys

args = sys.argv[1:]
append = "--append" in args
args = [a for a in args if a != "--append"]
path = args[0]
label = args[1] if len(args) > 1 else path
NUMS = re.compile(r":\d+(?:-\d+)?")


def masked(lines):
    return [NUMS.sub(":N", x) for x in lines]


def by_token(o, b, t, at):
    merged, notes = [], []
    for k, (lo, lb, lt) in enumerate(zip(o, b, t)):
        pick = []
        for x, y, z in zip(NUMS.findall(lo), NUMS.findall(lb), NUMS.findall(lt)):
            if x == z:
                pick.append(x)
                if x != y:
                    notes.append(f"line {at + k}: base {y} / ours {x} / theirs {z} -> {x} (both, alike)")
                continue
            rule, m = ("theirs", z) if x == y else ("ours", x) if z == y else ("both", x)
            pick.append(m)
            notes.append(f"line {at + k}: base {y} / ours {x} / theirs {z} -> {m} ({rule})")
        parts = NUMS.split(lo)
        merged.append(parts[0] + "".join(p + s for p, s in zip(pick, parts[1:])))
    return merged, notes


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
    if masked(o) == masked(t) and masked(o) == masked(b):
        merged, notes = by_token(o, b, t, at)
        said.append(f"{label}:{at}: numbers, {len(o)} line(s); " + "; ".join(notes))
        out.extend(merged)
    elif masked(o) == masked(t):
        pairs = [(x, y) for p, q in zip(o, t) for x, y in zip(NUMS.findall(p), NUMS.findall(q)) if x != y]
        said.append(f"{label}:{at}: numbers (both rewrote the base alike), {len(o)} line(s); ours kept; " + ", ".join(f"ours {x} / theirs {y}" for x, y in pairs))
        out.extend(o)
    elif masked(o) == masked(b):
        pairs = [(x, y) for p, q in zip(b, o) for x, y in zip(NUMS.findall(p), NUMS.findall(q)) if x != y]
        said.append(f"{label}:{at}: rewrite -- theirs taken ({len(b)} base lines -> {len(t)}); ours had moved only " + ", ".join(f"{x} -> {y}" for x, y in pairs))
        out.extend(t)
    elif masked(t) == masked(b):
        pairs = [(x, y) for p, q in zip(b, t) for x, y in zip(NUMS.findall(p), NUMS.findall(q)) if x != y]
        said.append(f"{label}:{at}: ours -- ours kept ({len(b)} base lines -> {len(o)}); theirs had moved only " + ", ".join(f"{x} -> {y}" for x, y in pairs))
        out.extend(o)
    elif append and not b and o and t:
        said.append(f"{label}:{at}: append -- both inserted at one base place: ours' {len(o)} lines, then theirs' {len(t)}")
        out.extend(o + t)
    else:
        sys.exit(f"{label}:{i + 1}: no rule fits this block -- read by hand")
    i = end + 1
open(path, "w", encoding="utf-8").write("\n".join(out))
print("\n".join(said))
print(f"# {label}: {len(said)} blocks resolved")
