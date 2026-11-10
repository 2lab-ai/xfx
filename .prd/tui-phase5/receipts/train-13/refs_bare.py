#!/usr/bin/env python3
# Train 13: train 12's refs_bare.py, unchanged but for this line.
"""Bare continuations, `:N` or `:N-M` with no path of their own, that follow
a `path:N` reference on the same line or one of the four lines after it
(`src/tui/shell.rs:14585-14610, the full transcript at :14621-14623`) --
the form refs.py, refs_all.py and refs_short.py do not read (opus, train 9).
In every tracked file outside `.prd/`, each bare continuation is given the
path of the nearest reference before it, and, for each side whose own copy
of the citing file holds the same continuation, mapped from that side's copy
of the path to the head's (`git diff -U0`, refs_all.py's `mapped`); one that
maps elsewhere is MOVED, one inside a changed hunk CHANGED. Only paths some
side changed against the head are read; a `@pin` after the number skips it.

    python3 -I -B refs_bare.py <repo> <head> <side>=<commit>...
"""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from refs_all import git, show, hunks, mapped  # noqa: E402

# Any `<name>.<ext>:N` -- fx's `.zig` files and the e2e `.ts` ones included --
# is the anchor for what follows it; only one that names a tracked file of
# this repository (by its path, or by a basename one tracked file has) is read.
PATHED = re.compile(r"(?<![A-Za-z0-9_./-])([A-Za-z0-9_./-]*[A-Za-z0-9_-]\.[A-Za-z]{1,5}):(\d+)")
BARE = re.compile(r"(?<![A-Za-z0-9_./`-])(?<!\d)`?:(\d+)(?:-(\d+))?(?![\d@]|-\d)")


def main():
    repo, head, sides = sys.argv[1], sys.argv[2], [a.split("=", 1) for a in sys.argv[3:]]
    tracked = [f for f in git(repo, "ls-tree", "-r", "--name-only", head).splitlines() if not f.startswith(".prd/")]
    tracked_set = set(tracked)
    by_name = {}
    for f in tracked:
        by_name.setdefault(os.path.basename(f), []).append(f)
    changed = set()
    for _name, commit in sides:
        changed.update(git(repo, "diff", "--name-only", commit, head).splitlines())
    files = [f for f in tracked if re.search(r"\.(rs|sh|py|md|toml|ts|json|yml)$", f)]
    total = flagged = 0
    print(f"# head {head}; sides: " + ", ".join(f"{n} {c}" for n, c in sides))
    for citing in files:
        lines = (show(repo, head, citing) or "").splitlines()
        anchor, anchor_at = None, -10
        seen = set()
        for number, row in enumerate(lines, 1):
            events = sorted([(m.start(), "path", m) for m in PATHED.finditer(row)]
                            + [(m.start(), "bare", m) for m in BARE.finditer(row)], key=lambda e: e[0])
            for _pos, kind, m in events:
                if kind == "path":
                    name = m.group(1)
                    if "/" in name:
                        anchor = name if name in tracked_set else None
                    else:
                        paths = by_name.get(name, [])
                        anchor = paths[0] if len(paths) == 1 else None
                    anchor_at = number
                    continue
                if anchor is None or number - anchor_at > 4 or anchor not in changed:
                    continue
                first, last = int(m.group(1)), m.group(2)
                spelled = m.group(0).lstrip("`")
                key = (anchor, spelled)
                if key in seen or show(repo, head, anchor) is None:
                    continue
                seen.add(key)
                total += 1
                cells = []
                for name, commit in sides:
                    mine = show(repo, commit, citing) or ""
                    if spelled not in mine or show(repo, commit, anchor) is None:
                        continue
                    spans = hunks(repo, commit, head, anchor)
                    ends = [mapped(first, spans)] + ([mapped(int(last), spans)] if last else [])
                    if any(e is None for e in ends):
                        cells.append(f"{name} CHANGED")
                    elif ends[0] != first or (last and ends[-1] != int(last)):
                        cells.append(f"{name} MOVED -> :" + "-".join(str(e) for e in ends))
                if cells:
                    flagged += 1
                    print(f"{citing}:{number}: {spelled} (after {anchor})\n    " + " | ".join(cells))
    print(f"# {total} bare continuations after a reference into a changed file; {flagged} moved or changed on some side that holds them")


main()
