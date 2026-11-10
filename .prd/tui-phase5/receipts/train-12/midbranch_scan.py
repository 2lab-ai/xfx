#!/usr/bin/env python3
"""Train 12, join check 7: the brief's path check over every commit of
<base>..<head> -- the units' own commits and this train's -- each read as
`git show --format=` prints it (a merge as its combined diff), and every hit
classified. The pattern is the brief's, each part spelt with a bracket class
so this file does not match itself; the account's name and uid are read at
run time (`pwd`, `os.getuid`), never written here, and never printed: a hit
is printed as its kind and the file it is in, with a placeholder's own name
or digits only where they are not this machine's.

Kinds (round 2: only what is documented is a placeholder):
  placeholder  a home the local-path check documents as nobody's (its
               `allowed` list, read from <head>'s check-no-local-paths.sh:
               `u`, `me`, `example`, `someone`, `you`, `runner`), or
               `claude-<n>/` inside a name or after a `/` (a model path such
               as `models/claud[e]-3/`, the forms the check passes)
  review       any other home, or a harness temp root whose uid is neither
               this machine's nor documented: listed by name for a reader
  SCRATCH      this machine's harness temp root (its uid), the macOS
               per-user temp root, or `claude-<uid>/` standing as a root's
               broken tail
  NAME         the account's name, or its home: stops the train
  CREDENTIAL   a credential's shape (a GitHub, OpenAI-style, AWS, Slack or
               Vercel token, a private key's armour): stops the train
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
CRED = re.compile(r"g[h][pousr]_[A-Za-z0-9]{30,}|githu[b]_pat_[A-Za-z0-9_]{40,}|s[k]-[A-Za-z0-9_-]{30,}|AKI[A][0-9A-Z]{16}"
                  r"|-----BEGI[N] [A-Z ]*PRIVATE KEY|xo[x][baprs]-[A-Za-z0-9-]{10,}|vc[k]_[A-Za-z0-9]{20,}")
ME = pwd.getpwuid(os.getuid()).pw_name
UID = str(os.getuid())


def git(repo, *a):
    return subprocess.run(["git", "-C", repo, *a], check=True, capture_output=True, text=True, errors="replace").stdout


ALLOWED = set()


def kind(line, m):
    home, tmp1, tmp2, short = m.group(1), m.group(2), m.group(3), m.group(4)
    if home is not None:
        name = re.match(r"[A-Za-z0-9._-]*", home).group(0)  # the home's own name, not what the pattern ran on to
        if name == ME or home == ME:
            return ("NAME", "the account's home")
        if f"/Users/{name}" in ALLOWED:
            return ("placeholder", f"a documented home named {name!r}")
        return ("review", f"a home named {name!r}, not in the check's documented list")
    if tmp1 is not None or tmp2 is not None:
        uid = tmp1 or tmp2
        return ("SCRATCH", "the harness temp root, this machine's uid") if uid == UID else ("review", f"a harness temp root, uid {uid}")
    if short is not None:
        before = line[m.start() - 1] if m.start() > 0 else ""
        if before == "/" or before.isalnum() or before in "._-":
            return ("placeholder", f"`claud[e]-{short}/` inside a name or after a `/`")
        return ("SCRATCH", "a root's tail, this machine's uid") if short == UID else ("review", f"a root's tail, uid {short}")
    return ("SCRATCH", "the macOS per-user temp root")


repo, base, head = sys.argv[1:4]
block = git(repo, "show", f"{head}:scripts/check-no-local-paths.sh").split("allowed=(", 1)[1].split("\n)", 1)[0]
ALLOWED = {m.group(1) for m in re.finditer(r"^\s*(/(?:home|Users)/[A-Za-z0-9._-]+)", block, re.M)}
print(f"# the check's documented homes at {head[:8]}: {', '.join(sorted(ALLOWED))}")
totals = {"placeholder": 0, "review": 0, "SCRATCH": 0, "NAME": 0, "CREDENTIAL": 0}
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
        if CRED.search(line):
            hits.append((path, "CREDENTIAL", "a credential's shape"))
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
rt = {"placeholder": 0, "review": 0, "SCRATCH": 0, "NAME": 0, "CREDENTIAL": 0}
hits, lines = scan(git(repo, "diff", f"{base}..{head}"))
show(hits, rt)
print(f"# the range: {lines} lines hit (the brief's count); matches: " + ", ".join(f"{k} {v}" for k, v in rt.items()))
sys.exit(1 if totals["NAME"] or totals["CREDENTIAL"] or names or rt["NAME"] or rt["CREDENTIAL"] else 0)
