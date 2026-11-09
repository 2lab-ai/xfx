#!/usr/bin/env python3
"""Train 12, join check 7: the brief's path check over every commit of
<base>..<head> -- the units' own commits and this train's -- each read as
`git show --format=` prints it (a merge as its combined diff), and every hit
classified. The pattern is the brief's, each part spelt with a bracket class
so this file does not match itself; the account's name and uid are read at
run time (`pwd`, `os.getuid`), never written here, and never printed: a hit
is printed as its kind and the file it is in, with a placeholder's own name
or digits only where they are not this machine's.

Kinds:
  placeholder  a home whose name is not the account's (`me`, `someone`, ...)
               or a harness temp root whose uid is not this machine's
               (`claude-0`), or `claude-<n>/` inside a name or after a `/`
               (a model path such as `models/claud[e]-3/`)
  SCRATCH      this machine's harness temp root (its uid), the macOS
               per-user temp root, or `claude-<uid>/` standing as a root's
               broken tail
  NAME         the account's name, or its home: stops the train
A line that ends in the local-path check's `local-path-ok` mark says so.

Then the same over the range, `git diff <base>..<head>`'s added lines: what
the head holds that <base> did not.

    python3 -I -B midbranch_scan.py <repo> <base> <head>
"""
import os
import pwd
import re
import subprocess
import sys

P = re.compile(r"/User[s]/([^/<]+)/|/privat[e]/tm[p]/claude-([0-9]+)|/tm[p]/claude-([0-9]+)|/var/folder[s]/|claud[e]-([0-9]+)/")
ME = pwd.getpwuid(os.getuid()).pw_name
UID = str(os.getuid())


def git(repo, *a):
    return subprocess.run(["git", "-C", repo, *a], check=True, capture_output=True, text=True, errors="replace").stdout


def kind(line, m):
    home, tmp1, tmp2, short = m.group(1), m.group(2), m.group(3), m.group(4)
    if home is not None:
        return ("NAME", "the account's home") if home == ME else ("placeholder", f"a home named {home!r}")
    if tmp1 is not None or tmp2 is not None:
        uid = tmp1 or tmp2
        return ("SCRATCH", "the harness temp root, this machine's uid") if uid == UID else ("placeholder", f"a harness temp root, uid {uid}")
    if short is not None:
        before = line[m.start() - 1] if m.start() > 0 else ""
        if before == "/" or before.isalnum() or before in "._-":
            return ("placeholder", f"`claud[e]-{short}/` inside a name or after a `/`")
        return ("SCRATCH", "a root's tail, this machine's uid") if short == UID else ("placeholder", f"a root's tail, uid {short}")
    return ("SCRATCH", "the macOS per-user temp root")


repo, base, head = sys.argv[1:4]
totals = {"placeholder": 0, "SCRATCH": 0, "NAME": 0}
total_lines = 0


def scan(text):
    path, hits, lines = None, [], 0
    for line in text.split("\n"):
        if line.startswith("+++ "):
            path = line[6:] if line.startswith("+++ b/") else line[4:]
            continue
        if not line.startswith("+"):
            continue
        lines += bool(P.search(line))
        mark = "; the line marked `local-path-ok`" if "local-path-ok" in line else ""
        for m in P.finditer(line):
            k, what = kind(line, m)
            hits.append((path, k, what + mark))
        if ME in line:
            hits.append((path, "NAME", "the account's name"))
    return hits, lines


def show(hits, totals):
    for path, k, what in hits:
        totals[k] += 1
        print(f"    {k:11} {what}  ({path})")


commits = git(repo, "rev-list", "--reverse", f"{base}..{head}").split()
for c in commits:
    hits, lines = scan(git(repo, "show", "--format=", c))
    if not hits:
        continue
    subject = git(repo, "log", "-1", "--format=%s", c).strip()[:64]
    print(f"{c[:8]} lines {lines} (the brief's count), matches {len(hits)}  {subject}")
    total_lines += lines
    show(hits, totals)
names = sum(ME in git(repo, "log", "-1", "--format=%B", c) for c in commits)
print(f"# {len(commits)} commits scanned; {total_lines} lines hit (the brief's count, summed); matches: " + ", ".join(f"{k} {v}" for k, v in totals.items())
      + f"; commit messages holding the account's name: {names}")
print()
print(f"## the range, git diff {base}..{head}")
rt = {"placeholder": 0, "SCRATCH": 0, "NAME": 0}
hits, lines = scan(git(repo, "diff", f"{base}..{head}"))
show(hits, rt)
print(f"# the range: {lines} lines hit (the brief's count); matches: " + ", ".join(f"{k} {v}" for k, v in rt.items()))
sys.exit(1 if totals["NAME"] or names or rt["NAME"] else 0)
