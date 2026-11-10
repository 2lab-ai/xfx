#!/usr/bin/env python3
"""Train 13's re-points (train 12's repoint.py, its plan this train's, and
its frame read by refs_content.py): every reference outside the historical
plans (`docs/superpowers/plans/`) whose cited line the merges moved -- found
by refs.py, refs_all.py, refs_short.py, refs_bare.py and refs_prd.py at the
merges' head bbdde09c (`refs-*-pre.txt`) and by refs_content.py there
(`content-pre.txt`) -- and two ledger refs that were wrong at the base
already (named below). CODE_HEAD is the join tests' commit ea35d548, which
appends two tests at their modules' ends and moves no cited line.

Each reference's frame is the commit that wrote its numbers
(refs_content.py's `frame`: `git blame` of the citing line at the code
head, followed through a merge into the parent that holds the ref, and
through a numbers-only edit that left it as it was). Its number is mapped
from that frame to the code head through `git diff -U0` (refs_all.py's
`mapped`), and the frame's cited text (and two lines either side) must stand
at exactly that place at the code head -- the two must agree, else the
script stops.

One entry has its expected text from APPROVAL-VERTICAL-SPELLINGS' refs.py
table instead (`EXPECTED`): the bare `:1747` of `amendment-card-after-stop`,
which train 11's re-point (6ae87955) wrote wrong -- its frame holds a doc
comment there, not `queue_card` -- so no frame can carry it; refs.py's
`QUEUE` text names the line.

    python3 -I -B repoint.py <repo> --derive   # prints each one: frame, old, new
    python3 -I -B repoint.py <repo> --apply    # writes the new numbers into the worktree, numbers only

repointed.py then checks each, from the numbers the head's text holds.
"""
import os
import re
import subprocess
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
import refs_content as rc  # noqa: E402
from refs_all import hunks, mapped  # noqa: E402

CODE_HEAD = "ea35d548"
L = "scripts/fxcap/parity-ledger.toml"
P = "docs/parity.md"
CAP = "scripts/fxcap/capfile.py"
SMOKE = "scripts/smoke-tui.sh"
SH = "src/tui/shell.rs"
# (citing file, line of the citing text at the code head, reference as written, why it is here)
PLAN = [
    (L, 107, ".prd/tui-phase4/receipts/d1/README.md:258", "wrong at the base f53606d1 already"),
    (L, 505, "src/tui/shell.rs:13866", "merges"),
    (L, 689, "src/tui/shell.rs:16840", "merges"),
    (L, 806, "src/tui/shell.rs:41113", "merges"),
    (L, 817, "src/tui/shell.rs:41113", "merges"),
    (L, 829, "src/tui/shell.rs:41113", "merges"),
    (L, 840, "src/tui/shell.rs:41113", "merges"),
    (L, 851, "src/tui/shell.rs:41113", "merges"),
    (L, 863, "src/tui/shell.rs:10480", "merges"),
    (L, 865, "src/tui/shell.rs:10480", "merges"),
    (L, 865, ":1747", "wrong at the base f53606d1 already (train 11's re-point)"),
    (L, 919, "src/tui/shell.rs:41113", "merges"),
    (L, 989, "src/tui/shell.rs:16840", "merges"),
    (L, 1066, "src/tui/shell.rs:4541", "merges"),
    (P, 48, ":1364-1375", "merges"),
    (P, 162, "scripts/smoke-tui.sh:10462-10477", "merges"),
    (P, 162, ":10514-10524", "merges"),
    (P, 237, "scripts/smoke-tui.sh:10462-10477", "merges"),
    (P, 237, ":10514-10524", "merges"),
    (CAP, 165, "src/tui/shell.rs:15057-15082", "merges"),
    (CAP, 166, ":15075-15077", "merges"),
    (SMOKE, 11213, "input.rs:1063", "merges"),
    (SH, 24555, "shell.rs:6070", "merges"),
    (SH, 24556, ":6258", "merges"),
]
# (citing, line, token) -> the cited line's text, stripped (APPROVAL-VERTICAL-SPELLINGS' refs.py `QUEUE`)
EXPECTED = {(L, 865, ":1747"): "fn queue_card(transcript: &mut Transcript, text: &str) {"}
# A bare ref on a line with no ref of its own continues the previous line's last ref (refs_bare.py's rule).
CONTINUES = {(CAP, 166): (CAP, 165), (SH, 24556): (SH, 24555)}


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
