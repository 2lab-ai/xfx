#!/usr/bin/env python3
"""Train 12's re-points (train 11's repoint.py, its plan and sides this
train's): every `path:line`, `<basename>:line` and bare `:line` reference
outside the historical plans (`docs/superpowers/plans/`, REF-LINT's, as
trains 8 to 11 left them) whose cited line the merges moved (refs.py,
refs_all.py, refs_short.py, refs_bare.py and train 12's refs_prd.py at the
merges' head 12b56ab8, before any re-point; `refs-*-pre.txt`).

Each reference is read in the frame of the side that wrote the line citing
it: `git blame` of that line at the code head names a commit, and the frame
is the tip of the side that holds it -- main df46777e, or a unit's tip. Its
number is mapped from that frame to the code head through `git diff -U0`
(refs_all.py's `mapped`); a number that falls inside a changed hunk stops
the script.

    python3 -I -B repoint.py <repo> --derive   # prints each one: frame, old, new
    python3 -I -B repoint.py <repo> --apply    # writes the new numbers into the worktree, numbers only

repointed.py then checks each, from the numbers the head's text holds.
"""
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from refs_all import hunks, mapped  # noqa: E402

CODE_HEAD = "12b56ab8"
SIDES = [("main", "df46777e"), ("gate-env-maintlock", "a1cbdcaf"), ("sweep-wallclock", "4bc9ab13"),
         ("local-paths", "5609108f"), ("input-row-limit", "3d3d56c8")]
L = "scripts/fxcap/parity-ledger.toml"
SH = "src/tui/shell.rs"
CAP = "scripts/fxcap/capture.sh"
D1 = ".prd/tui-phase4/receipts/d1/README.md"
# (citing file, line of the citing text at the code head, reference as written, cited file)
PLAN = [
    (L, 78, "scripts/fxcap/capture.sh:659", CAP),
    (L, 85, "scripts/fxcap/capture.sh:314-316", CAP),
    (L, 85, "scripts/fxcap/capture.sh:367-373", CAP),
    (L, 85, "scripts/fxcap/capture.sh:947", CAP),
    (L, 85, "scripts/fxcap/capture.sh:954", CAP),
    (L, 147, ".prd/tui-phase4/receipts/d1/README.md:145", D1),
    (L, 147, "scripts/fxcap/capture.sh:987", CAP),
    (L, 816, "src/tui/shell.rs:40879", SH),
    (L, 827, "src/tui/shell.rs:40879", SH),
    (L, 839, "src/tui/shell.rs:40879", SH),
    (L, 850, "src/tui/shell.rs:40879", SH),
    (L, 861, "src/tui/shell.rs:40879", SH),
    (L, 929, "src/tui/shell.rs:40879", SH),
]


def git(repo, *a):
    return subprocess.run(["git", "-C", repo, *a], check=True, capture_output=True, text=True).stdout


def token_re(token):
    # the token whole: not inside a longer path or number
    return re.compile(r"(?<![A-Za-z0-9_./-])" + re.escape(token) + r"(?![\d-]|-\d)")


def line_of(repo, path, at, token):
    text = git(repo, "show", f"{CODE_HEAD}:{path}").split("\n")
    found = [i + 1 for i, l in enumerate(text) if token_re(token).search(l)]
    if at is None:
        if len(found) != 1:
            sys.exit(f"{path}: {token} on {len(found)} lines {found}; name the line")
        return found[0]
    if at not in found:
        sys.exit(f"{path}:{at} does not hold {token} (it is on {found})")
    return at


def frame_of(repo, path, at):
    blame = git(repo, "blame", "--porcelain", "-L", f"{at},{at}", CODE_HEAD, "--", path).split()[0]
    holders = [n for n, c in SIDES
               if subprocess.run(["git", "-C", repo, "merge-base", "--is-ancestor", blame, c]).returncode == 0]
    if not holders:
        sys.exit(f"{path}:{at}: written by {blame[:8]}, which no side holds")
    return blame, holders


def derive(repo):
    out = []
    for citing, at, token, cited in PLAN:
        at = line_of(repo, citing, at, token)
        blame, holders = frame_of(repo, citing, at)
        # main holds everything a merge base holds; prefer main when it holds the line
        side = "main" if "main" in holders else holders[0]
        commit = dict(SIDES)[side]
        m = re.search(r":(\d+)(?:-(\d+))?$", token)
        nums = [int(m.group(1))] + ([int(m.group(2))] if m.group(2) else [])
        spans = hunks(repo, commit, CODE_HEAD, cited)
        new = [mapped(n, spans) for n in nums]
        if any(n is None for n in new):
            sys.exit(f"{citing}:{at}: {token} falls in a changed hunk of {cited} ({side} -> head)")
        new_token = token[: m.start()] + ":" + "-".join(str(n) for n in new)
        out.append((citing, at, token, new_token, cited, side, commit, blame, holders))
    return out


def main():
    repo, mode = sys.argv[1], sys.argv[2]
    rows = derive(repo)
    if mode == "--derive":
        for citing, at, token, new, cited, side, commit, blame, holders in rows:
            print(f"{citing}:{at}: {token} -> {new}   (line written by {blame[:8]}, held by {', '.join(holders)}; "
                  f"frame {side} {commit})")
        print(f"# {len(rows)} references; {sum(t != n for _, _, t, n, *_ in rows)} with a new number")
        return
    assert mode == "--apply"
    edits = {}
    for citing, at, token, new, *_ in rows:
        edits.setdefault(citing, []).append((at, token, new))
    for citing, items in edits.items():
        path = os.path.join(repo, citing)
        lines = open(path, encoding="utf-8").read().split("\n")
        for at, token, new in items:
            line = lines[at - 1]
            hits = token_re(token).findall(line)
            if len(hits) != 1:
                sys.exit(f"{citing}:{at}: {token} found {len(hits)} times in the worktree's line")
            lines[at - 1] = token_re(token).sub(new, line, count=1)
        open(path, "w", encoding="utf-8").write("\n".join(lines))
        print(f"{citing}: {len(items)} re-pointed")


main()
