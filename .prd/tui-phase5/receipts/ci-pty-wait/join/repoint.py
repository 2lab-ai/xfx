#!/usr/bin/env python3
"""The join with train 16: the references this merge moved, re-pointed,
numbers only.

Train 16's repoint.py does the planning (`plan`, `numbers`, imported from
its folder). Its HAND, EXTRA and TEXT tables are emptied (that train's
re-points, applied there); its LEAVE table is kept (refs it left WRONG on
purpose, or pinned to a commit's file), and the two tables below are this
join's. The input is the content check (train 16's content.sh, or its
refs_content.py for a file under `.prd/`) at the merge and at each parent.
A WRONG ref at the merge is the merge's (`merges`) when neither parent's
check reads it WRONG with the same citing file, frame and numbers; else it
is that parent's, older than the join, and printed but not moved. Each
`merges` ref gets the number where its frame's cited lines, and two either
side, stand at exactly one place of the merge's cited file (`auto`), or
the cited lines alone (`lines`); a ref with neither stops the run, to be
read by hand.

usage: python3 -I -B repoint.py <repo> <merge> --content <file>
           [--side <name>=<file>]... [--apply]
--apply writes the `merges` refs' new numbers into the worktree, each into
its own token (train 16's --apply, restricted to them). Prints no path.
"""
import os
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, "..", "..", "train-16"))
import refs_content as rc  # noqa: E402
import repoint as rp  # noqa: E402

for table in (rp.HAND, rp.EXTRA, rp.TEXT):
    table.clear()
# Read by hand: two numbers of `tests/tui.rs:2721`'s comma list (train 16's
# re-point at its head, adb809f5), each `session.type_bytes(&[0x15, 0x04]);`
# -- a line that stands at many places, so no window places it. Mapped
# through the diff from 8be4693b's tests/tui.rs to the merge's (difflib,
# an unchanged block): +270, as the list's other numbers past this branch's
# hostile-paste lines.
for _old, _new in ((",4049", "4319"), (",9331", "9601")):
    rp.hand("tests/tui.rs", _old, "list", _new, "session.type_bytes(&[0x15, 0x04]);",
            "the merge put this branch's 270 lines above it")
# This receipt's README cites, as CI printed them, the failing assertion's
# line in the jobs' own code (`ci/excerpts/`): not the head's, so not moved.
for _ref in ("tests/tui.rs:852", "tests/tui.rs:857"):
    rp.LEAVE[(".prd/tui-phase5/receipts/ci-pty-wait/README.md", _ref, "path")] = (
        "the line CI printed for the jobs it names (ci/excerpts/), in their code")
# The README's round-2 table (its lines 329-335) is pinned: its header says
# where its refs are, `62ea9f56`, and the content check, blaming rows round
# 3 rewrote, reads some of them against ef595df2 instead. Every ref there is
# left, but the two round 3 wrote into row 331's text, which name lib tests
# 62ea9f56 does not have, and are at round 3's head like the rest.
README = ".prd/tui-phase5/receipts/ci-pty-wait/README.md"
PINNED = {(README, n) for n in range(329, 336)}
UNPINNED = {(README, 331, "src/tui/input.rs:3549"), (README, 331, ":4148")}
# WRONG on this branch before the join, and this branch's: its own lines
# above the cited ones moved them (loop_turn), and 8be4693b reads it SAME.
MINE = {("src/tui/event_loop.rs", "event_loop.rs:1128-1131", "short")}

args = sys.argv[1:]
repo, head = args[0], args[1]
content = args[args.index("--content") + 1]
sides = [tuple(args[i + 1].split("=", 1)) for i, a in enumerate(args) if a == "--side"]
out, missing = rp.plan(repo, head, content, sides)
pinned_unplaced = len(missing)
missing = [(w, origin) for w, origin in missing
           if (w["citing"], w["n"]) not in PINNED or (w["citing"], w["n"], w["spelled"]) in UNPINNED]
pinned_unplaced -= len(missing)
for w, origin in missing:
    print(f"NO PLAN {origin} {w['citing']}:{w['n']} {w['spelled']} ({w['kind']}; frame {w['frame']} :{w['f0']})")
counts = {}
moved = []
for w, origin, path, new, how, why in out:
    counts[origin] = counts.get(origin, 0) + 1
    to = rp.numbers(w["spelled"], new) if new else "(none)"
    mine = origin == "merges" or (w["citing"], w["spelled"], w["kind"]) in MINE
    if (w["citing"], w["n"]) in PINNED and (w["citing"], w["n"], w["spelled"]) not in UNPINNED:
        mine, how = False, "pinned"
    verb = "MOVE" if mine and new else "keep"
    print(f"{verb} {how:5} {origin:7} {w['citing']}:{w['n']} {w['spelled']} -> {to} "
          f"(cites {path}; frame {w['frame']} :{w['f0']}{'-' + str(w['l0']) if w['l0'] else ''})")
    if mine and new:
        moved.append((w, new))
print(f"# {len(out) + len(missing) + pinned_unplaced} WRONG at {head}: "
      + "; ".join(f"{k} {v}" for k, v in sorted(counts.items()))
      + f"; pinned, no number found {pinned_unplaced}; no plan {len(missing)}; to move {len(moved)}")
if missing:
    sys.exit(1)
if "--apply" in args:
    edits = {}
    for w, new in moved:
        edits.setdefault(w["citing"], {}).setdefault(w["n"], []).append((w, new))
    for citing, by_line in edits.items():
        lines = open(os.path.join(repo, citing), encoding="utf-8").read().split("\n")
        for n, items in by_line.items():
            line = lines[n - 1]
            found = rc.refs_of(line)
            taken, spans = set(), []
            for w, new in items:
                i = next(i for i, r in enumerate(found) if i not in taken
                         and line[r[0]:r[1]] == w["spelled"] and r[5] == w["kind"])
                taken.add(i)
                spans.append((found[i][0], found[i][1], rp.numbers(w["spelled"], new)))
            for a, b, text in sorted(spans, reverse=True):
                line = line[:a] + text + line[b:]
            lines[n - 1] = line
        open(os.path.join(repo, citing), "w", encoding="utf-8").write("\n".join(lines))
    print(f"# applied {len(moved)} numbers in {len(edits)} files")
