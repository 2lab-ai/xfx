#!/usr/bin/env python3
"""Train 11: a file filecheck.py reads False from one end only because
`git diff` grouped one side's insertion differently from that end: a run
inserted next to a base line the run itself also holds (worker.rs: train
10's tests appended after a base `        );`, a line those tests hold too), so
the diff can align the run on either side of it. For such a file, the
hunks that matched none are taken from both views -- `<base> -> <side>` and
`<other> -> <merged>` -- and must remove the same base lines (joined in
order); then, from the first unmatched hunk's start in each (the side's
file, the merge), the longer of the two spans the unmatched hunks cover
(first hunk's start to last hunk's end, the unchanged lines between
included) must be the same lines in both, line for line.

    python3 -I -B regroup.py <repo> <base> <side> <other> <merged|-> <path>

`-` for merged reads the worktree's file. Exit 0 when both hold.
"""
import re
import subprocess
import sys

repo, base, side, other, merged, path = sys.argv[1:7]
HEAD = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@")


def hunks(a, b):
    cmd = ["git", "-C", repo, "diff", "-U0", "--no-color", a] + ([b] if b != "-" else []) + ["--", path]
    out, cur = [], None
    for line in subprocess.run(cmd, check=True, capture_output=True, text=True).stdout.split("\n"):
        m = HEAD.match(line)
        if m:
            start, n = int(m.group(1)), int(m.group(2) if m.group(2) is not None else 1)
            cur = {"start": start if n else start + 1, "n": n, "lines": []}
            out.append(cur)
        elif cur is not None and line[:1] in ("+", "-") and not line.startswith(("+++ ", "--- ")):
            cur["lines"].append(line)
    return out


def text(c):
    if c == "-":
        return open(f"{repo}/{path}", encoding="utf-8").read().split("\n")
    return subprocess.run(["git", "-C", repo, "show", f"{c}:{path}"], check=True, capture_output=True,
                          text=True).stdout.split("\n")


mine, theirs_view = hunks(base, side), hunks(other, merged)
mine_bodies = [h["lines"] for h in mine]
view_bodies = [h["lines"] for h in theirs_view]
um = [h for h in mine if h["lines"] not in view_bodies]
uv = [h for h in theirs_view if h["lines"] not in mine_bodies]
removed = lambda hs: [l for h in hs for l in h["lines"] if l.startswith("-")]  # noqa: E731
same_removed = removed(um) == removed(uv)
span = lambda hs: (hs[0]["start"], hs[-1]["start"] + hs[-1]["n"]) if hs else (0, 0)  # noqa: E731
(a0, a1), (b0, b1) = span(um), span(uv)
n = max(a1 - a0, b1 - b0)
region_side, region_merged = text(side)[a0 - 1:a0 - 1 + n], text(merged)[b0 - 1:b0 - 1 + n]
same_region = bool(um) and region_side == region_merged
print(f"{path}: unmatched hunks: {len(um)} of {side[:8]}'s against the base, {len(uv)} of the merge's against "
      f"{other[:8]}; the same base lines removed: {same_removed} ({len(removed(um))}); "
      f"{side[:8]}'s lines {a0}-{a0 + n - 1} and the merge's {b0}-{b0 + n - 1} the same, line for line: "
      f"{same_region} ({n} lines; the spans {a1 - a0} and {b1 - b0})")
sys.exit(0 if same_removed and same_region else 1)
