#!/usr/bin/env python3
"""The join of main into train 13: the references the join moved, re-pointed
(train 13's repoint.py, its plan this join's; it reads `../refs_content.py`
and `../refs_all.py`). The plan is every reference outside the historical
plans (`docs/superpowers/plans/`) that refs.py, refs_all.py, refs_short.py
or refs_bare.py flag MOVED at the merge 8adc57df (`refs-pre.txt`,
`refs_*-pre.txt`), each one on a line that names its file read WRONG there
by refs_content.py too (`content-8adc57df.txt`; a bare continuation on the
next line is outside what it reads), but one: test_sweep.py's
`cells.py:100000`, a line past the file's end on purpose. Each is moved by the other side's lines: the
train's `src/tui/shell.rs` numbers by main's (PR #91's version line and
train 12's CI rounds), main's two in tests/tui.rs by the train's.
refs_content.py's other WRONG at the merge are WRONG on both parents
already (`content-afd891af.txt`, `content-d2b2ca75.txt`) and are not moved
by the join: they stay, as train 13 left them.

Each reference's frame is the commit that wrote its numbers
(refs_content.py's `frame`: `git blame` of the citing line at the merge,
followed through a merge into the parent that holds the ref, and through a
numbers-only edit that left it): e1e15efd (train 13's re-point) for the
train's, 2e388ca8 (train 12's) for main's two. Its number is mapped from
that frame to the merge through `git diff -U0` (refs_all.py's `mapped`), and
the frame's cited text, and two lines either side, must stand at exactly
that place at the merge -- else the script stops. No entry takes its text
from a table this time: the bare `:1929` of `amendment-card-after-stop`,
which train 13 set by name, is `queue_card` in its frame e1e15efd, so it
maps like the rest.

    python3 -I -B repoint.py <repo> --derive   # prints each one: frame, old, new
    python3 -I -B repoint.py <repo> --apply    # writes the new numbers into the worktree, numbers only

repointed.py then checks each, from the numbers the head's text holds.
"""
import os
import re
import subprocess
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, ".."))
sys.path.insert(0, here)
import refs_content as rc  # noqa: E402
from refs_all import hunks, mapped  # noqa: E402

CODE_HEAD = "8adc57df"
L = "scripts/fxcap/parity-ledger.toml"
CAP = "scripts/fxcap/capfile.py"
SH = "src/tui/shell.rs"
T = "tests/tui.rs"
J = "moved by main's lines"
K = "moved by the train's lines"
# (citing file, line of the citing text at the merge, reference as written, why it is here)
PLAN = [
    (L, 506, "src/tui/shell.rs:13962", J),
    (L, 689, "src/tui/shell.rs:16977", J),
    (L, 806, "src/tui/shell.rs:42431", J),
    (L, 817, "src/tui/shell.rs:42431", J),
    (L, 829, "src/tui/shell.rs:42431", J),
    (L, 840, "src/tui/shell.rs:42431", J),
    (L, 851, "src/tui/shell.rs:42431", J),
    (L, 863, "src/tui/shell.rs:10558", J),
    (L, 865, "src/tui/shell.rs:10558", J),
    (L, 865, ":1929", J),
    (L, 919, "src/tui/shell.rs:42431", J),
    (L, 989, "src/tui/shell.rs:16977", J),
    (L, 1083, "src/tui/shell.rs:4557", J),
    (CAP, 180, "src/tui/shell.rs:15194-15219", J),
    (CAP, 181, ":15212-15214", J),
    (SH, 24557, "shell.rs:6086", J),
    (SH, 24558, ":6274", J),
    (T, 9331, "src/tui/shell.rs:16806", K),
    (T, 9371, "src/tui/shell.rs:9667", K),
]
# (citing, line, token) -> the cited line's text, stripped: none this join (repointed.py reads it)
EXPECTED = {}
# A bare ref on a line with no ref of its own continues the previous line's last ref (refs_bare.py's rule).
CONTINUES = {(CAP, 181): (CAP, 180), (SH, 24558): (SH, 24557)}


def token_re(token):
    # the token whole: not inside a longer path or number
    return re.compile(r"(?<![A-Za-z0-9_./-])" + re.escape(token) + r"(?![\d-]|-\d)")


def the_ref(citing, at, token):
    """refs_content.py's tuple for `token` on line `at`, the cited path, and
    the line whose blame is its frame (the line above, for a continuation)."""
    line = rc.show(CODE_HEAD, citing)[at - 1]
    found = [r for r in rc.refs_of(line) if line[r[0]:r[1]] == token]
    if len(found) == 1:
        return found[0], found[0][2], at
    if (citing, at) in CONTINUES and token.startswith(":"):
        pc, pa = CONTINUES[(citing, at)]
        above = rc.show(CODE_HEAD, pc)[pa - 1]
        joined = above + " " + line
        refs = [r for r in rc.refs_of(joined) if joined[r[0]:r[1]] == token and r[0] > len(above)]
        if len(refs) == 1:
            return refs[0], refs[0][2], pa
    sys.exit(f"{citing}:{at}: {token} read {len(found)} times as a ref")


def derive(repo):
    rc.setup(repo, CODE_HEAD)
    out = []
    for citing, at, token, why in PLAN:
        ref, cited, blame_at = the_ref(citing, at, token)
        m = re.search(r":(\d+)(?:-(\d+))?$", token)
        nums = [int(m.group(1))] + ([int(m.group(2))] if m.group(2) else [])
        b = rc.blame(CODE_HEAD, citing, blame_at)[blame_at]
        if (citing, at, token) in EXPECTED:
            text = rc.show(CODE_HEAD, cited)
            want = EXPECTED[(citing, at, token)]
            hits = [i + 1 for i, line in enumerate(text) if line.strip() == want]
            if len(hits) != 1:
                sys.exit(f"{citing}:{at}: {want!r} on {len(hits)} lines")
            new = [hits[0]]
            frame, how = b["commit"], f"the entry's text from refs.py's table, {want[:40]!r}"
        else:
            if blame_at == at:
                frame, f0, l0 = rc.frame(CODE_HEAD, citing, at, ref, b)
            else:  # a continuation: the frame of the line that names the file
                above_ref = rc.refs_of(rc.show(CODE_HEAD, citing)[blame_at - 1])[-1]
                frame, _, _ = rc.frame(CODE_HEAD, citing, blame_at, above_ref, b)
                f0, l0 = nums[0], (nums[1] if len(nums) > 1 else None)
            if [f0] + ([l0] if l0 else []) != nums:
                sys.exit(f"{citing}:{at}: {token}: the frame {frame[:8]} wrote :{f0}{'-' + str(l0) if l0 else ''}")
            spans = hunks(repo, frame, CODE_HEAD, cited)
            new = [mapped(n, spans) for n in nums]
            if any(n is None for n in new):
                sys.exit(f"{citing}:{at}: {token} falls in a changed hunk of {cited} ({frame[:8]} -> head)")
            wb, _ = rc.window(rc.show(frame, cited), nums[0], nums[1] if len(nums) > 1 else None)
            wh, _ = rc.window(rc.show(CODE_HEAD, cited), new[0], new[1] if len(new) > 1 else None)
            if wb != wh:
                sys.exit(f"{citing}:{at}: {token} -> :{new}: the frame's text and two either side are not there at the head")
            how = "mapped through git diff; the frame's text and two either side there"
        new_token = token[: m.start()] + ":" + "-".join(str(n) for n in new)
        out.append((citing, at, token, new_token, cited, frame, why, how))
    return out


def main():
    repo, mode = sys.argv[1], sys.argv[2]
    rows = derive(repo)
    if mode == "--derive":
        for citing, at, token, new, cited, frame, why, how in rows:
            print(f"{citing}:{at}: {token} -> {new}   ({cited}; frame {frame[:8]}; {why}; {how})")
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


if __name__ == "__main__":
    main()
