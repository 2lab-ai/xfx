#!/usr/bin/env python3
"""Short references, `<basename>:<line>` with no directory (`shell.rs:12275`,
`capture.sh:430`), into a file some side of the train changed, in every
tracked file outside `.prd/`: refs_all.py's check for the form it does not
read. A basename counts only when it names one tracked file (outside
`.prd/`) and that file changed between some side and the head; a reference
followed by `@` is a pin (another product's commit) and is skipped.

    python3 -I -B refs_short.py <repo> <head> <side>=<commit>...
"""
import functools
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from refs_all import git, show, hunks, mapped  # noqa: E402

SHORT = re.compile(r"(?<![A-Za-z0-9_./-])([A-Za-z0-9_-]+\.(?:rs|sh|py|ts)):(\d+)(?:-(\d+))?(?![\d@]|-\d|,\d)")


def main():
    repo, head, sides = sys.argv[1], sys.argv[2], [arg.split("=", 1) for arg in sys.argv[3:]]
    tracked = [f for f in git(repo, "ls-tree", "-r", "--name-only", head).splitlines() if not f.startswith(".prd/")]
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
        text = show(repo, head, citing) or ""
        for m in sorted(set((m.group(0), m.group(1), int(m.group(2)), m.group(3)) for m in SHORT.finditer(text))):
            spelled, base, first, last = m
            paths = by_name.get(base, [])
            if len(paths) != 1 or paths[0] not in changed:
                continue
            path = paths[0]
            total += 1
            cells = []
            for name, commit in sides:
                if spelled not in (show(repo, commit, citing) or "") or show(repo, commit, path) is None:
                    continue
                spans = hunks(repo, commit, head, path)
                ends = [mapped(first, spans)] + ([mapped(int(last), spans)] if last else [])
                if any(e is None for e in ends):
                    cells.append(f"{name} CHANGED")
                elif ends[0] != first or (last and ends[-1] != int(last)):
                    cells.append(f"{name} MOVED -> " + "-".join(str(e) for e in ends))
            if cells:
                flagged += 1
                print(f"{citing}: {spelled} ({path})\n    " + " | ".join(cells))
    print(f"# {total} short references into changed files; {flagged} moved or changed on some side that cites them")


main()
