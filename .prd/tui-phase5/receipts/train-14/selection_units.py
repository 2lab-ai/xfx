#!/usr/bin/env python3
"""Train 14 (new; train 13 swept the four its units had swept beyond its
selection in a second round, opus S2): the scenarios the units' own head
sweeps took -- each unit's committed scoreboard, read at <commit> -- that
the first selection (<selection>) does not, written as one `--only` name a
line with the units that swept it in a comment above. Every name is a
scenario at the head and at the base (checked; one that is not is listed
and left out).

    python3 -I -B selection_units.py <repo> <head> <base> <selection> <commit>:<scoreboard>...
"""
import fnmatch
import re
import subprocess
import sys

repo, head, base, first = sys.argv[1:5]
boards = sys.argv[5:]


def git(*a):
    return subprocess.run(["git", "-C", repo, *a], check=True, capture_output=True, text=True).stdout


def names(commit):
    return {p.rsplit("/", 1)[-1][:-4] for p in git("ls-tree", "--name-only", commit, "scripts/fxcap/scenarios/").split()
            if p.endswith(".cap")}


globs = [l.strip() for l in open(first) if l.strip() and not l.startswith("#")]
swept = {}
for spec in boards:
    commit, path = spec.split(":", 1)
    unit = path.split("/")[-2]
    for line in git("show", f"{commit}:{path}").split("\n"):
        m = re.match(r"^\| ([a-z0-9][a-z0-9_.-]*) \| ran \|", line)
        if m and m.group(1) != "total":
            swept.setdefault(m.group(1), set()).add(unit)
at_head, at_base = names(head), names(base)
out = sorted(s for s in swept if not any(fnmatch.fnmatch(s, g) for g in globs))
print(f"# train 14's units' selection: the {len(out)} scenarios the units' {len(boards)} committed head sweeps took "
      f"({len(swept)} in all) that {first.rsplit('/', 1)[-1]} does not")
for s in out:
    if s not in at_head or s not in at_base:
        print(f"# {s} ({', '.join(sorted(swept[s]))}): not a scenario at {'the head' if s not in at_head else 'the base'}, left out")
        continue
    print(f"# {s}: {', '.join(sorted(swept[s]))}")
    print(s)
