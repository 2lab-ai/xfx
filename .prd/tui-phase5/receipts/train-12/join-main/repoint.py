#!/usr/bin/env python3
"""The references the join of main moved, re-pointed in the frame that wrote
them (train 12's repoint.py, its plan this merge's).

    python3 -I -B repoint.py <repo> --derive [late]   # each one: frame, old, new, the code it names
    python3 -I -B repoint.py <repo> --apply [late]    # writes the new numbers into the worktree

The plan is every reference refs.py, refs_all.py, refs_short.py, refs_bare.py
and refs_prd.py flag MOVED at the merge 0c39ce8c outside the historical plans
(`docs/superpowers/plans/`; `*-pre.txt` here), but one: the ledger's
`capture.sh:987` at line 86 is the train's own (capture-tmpdir, re-pointed in
ea772025) and reads `=` in the train's frame; main's MOVED there is main's old
`:987` for tmux-dead-pane, which the train had already re-pointed to `:1157`.

`late` is the four refs_all.py flagged at the merge that the first plan
missed (its list was cut short when read), taken at c7d30afc, which applied
the first. test_sweep.py's `scripts/fxcap/cells.py:100000` is a line past
the file's end on purpose (the case that a reference to a line the file
lacks is refused) and stays. tests/sessions.rs's `src/app.rs:242` is mapped
from the commit that wrote it (7daab7f9), where it named
`SessionDetailSnapshot::new(&detail).render(..)`; read in the merge base's
frame it lands on `Command::Sessions`, because src/app.rs moved between the
two and nothing re-pointed it. The two in tests/tui.rs named a struct field
and a `.collect()` at 6ae87955, the last commit to write them, and mapping
carries that into every frame, so they are set by name (`NAMED`):
`Shell::blind` is `fn blind` (e94af067, which wrote both, had `:2703` on
it). For the Redraw, e94af067's `:2015` was on neither candidate: three
lines above `act`'s arm (2018), far from `decide`'s (1400). The comment's
"inside the approval panel" and "answers nothing" are `decide`'s arm, which
handles the Redraw with the panel open and returns before `act` runs.

Each reference is read in the frame of the commit that last wrote its citing
line (`git blame` at the plan's commit). The first plan was applied reading
the both-sides ones in the merge base's frame; frames.py shows all 23 land on
the same numbers from the blamed commit, which is the frame now. Its
numbers are mapped from that frame to the plan's commit through
`git diff -U0`; a number inside a changed hunk stops the script. Then the
frame's cited lines, and two either side, must equal the head's at the new
numbers.
"""
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from refs_all import hunks, mapped  # noqa: E402

HEAD = "0c39ce8c"
TRAIN, MAIN, BASE = "71d3e02b", "8f921425", "df46777e"
L = "scripts/fxcap/parity-ledger.toml"
SH = "src/tui/shell.rs"
# (citing file, citing line at the merge, the reference as written, the cited file)
PLAN = [
    (L, 506, "src/tui/shell.rs:13866", SH),
    (L, 552, "src/tui/shell.rs:3820-3837", SH),
    (L, 553, "src/tui/shell.rs:3820-3837", SH),
    (L, 690, "src/tui/shell.rs:16840", SH),
    (L, 816, "src/tui/shell.rs:41113", SH),
    (L, 827, "src/tui/shell.rs:41113", SH),
    (L, 839, "src/tui/shell.rs:41113", SH),
    (L, 850, "src/tui/shell.rs:41113", SH),
    (L, 861, "src/tui/shell.rs:41113", SH),
    (L, 873, "src/tui/shell.rs:10480", SH),
    (L, 875, "src/tui/shell.rs:10480", SH),
    (L, 875, ":1747", SH),
    (L, 929, "src/tui/shell.rs:41113", SH),
    (L, 965, "src/tui/shell.rs:1854", SH),
    (L, 999, "src/tui/shell.rs:16840", SH),
    (L, 1093, "src/tui/shell.rs:4541", SH),
    ("scripts/fxcap/capfile.py", 180, "src/tui/shell.rs:15057-15082", SH),
    ("scripts/fxcap/capfile.py", 181, ":15075-15077", SH),
    ("scripts/smoke-tui.sh", 11212, "shell.rs:2705", SH),
    ("src/tui/event_loop.rs", 14833, "shell.rs:2817", SH),
    (SH, 23739, "shell.rs:6070", SH),
    (SH, 23740, ":6258", SH),
    ("docs/parity.md", 241, ".prd/01-spec.md:56", ".prd/01-spec.md"),
]
LATE_HEAD = "c7d30afc"
LATE = [
    ("tests/sessions.rs", 3239, "src/app.rs:242", "src/app.rs"),
]
# (citing file, line, reference, cited file, new line, what that line must hold)
NAMED = [
    ("tests/tui.rs", 9331, "src/tui/shell.rs:2814", SH, 16806, "pub(crate) fn blind(&self) -> bool {"),
    ("tests/tui.rs", 9371, "src/tui/shell.rs:2113", SH, 9667, "Input::Action(Action::Redraw) => {"),
]
if sys.argv[3:] == ["late"]:
    HEAD, PLAN = LATE_HEAD, LATE
else:
    NAMED = []


def git(repo, *args):
    return subprocess.run(["git", "-C", repo, *args], check=True, capture_output=True, text=True).stdout


def ancestor(repo, a, b):
    return subprocess.run(["git", "-C", repo, "merge-base", "--is-ancestor", a, b]).returncode == 0


def frame(repo, path, line):
    commit = git(repo, "blame", "--porcelain", "-L", "%d,%d" % (line, line), HEAD, "--", path).split()[0]
    in_train, in_main = ancestor(repo, commit, TRAIN), ancestor(repo, commit, MAIN)
    if in_train and in_main:
        return "base", BASE, commit
    return ("train", TRAIN, commit) if in_train else ("main", MAIN, commit)


def lines(repo, commit, path):
    return git(repo, "show", "%s:%s" % (commit, path)).split("\n")


def derive(repo):
    out = []
    for citing, at, ref, cited in PLAN:
        text = lines(repo, HEAD, citing)[at - 1]
        assert ref in text, (citing, at, ref)
        name, commit, blamed = frame(repo, citing, at)
        m = re.search(r":(\d+)(?:-(\d+))?$", ref)
        a, b = int(m.group(1)), int(m.group(2) or m.group(1))
        spans = hunks(repo, blamed, HEAD, cited)
        na, nb = mapped(a, spans), mapped(b, spans)
        assert na is not None and nb is not None, ("inside a changed hunk", citing, at, ref)
        old, new = lines(repo, blamed, cited), lines(repo, HEAD, cited)
        same = old[a - 3 : b + 2] == new[na - 3 : nb + 2]
        new_ref = ref[: m.start()] + (":%d-%d" % (na, nb) if m.group(2) else ":%d" % na)
        out.append((citing, at, ref, new_ref, name, blamed[:8], same, new[na - 1].strip()[:70]))
    for citing, at, ref, cited, n, holds in NAMED:
        assert ref in lines(repo, HEAD, citing)[at - 1], (citing, at, ref)
        code = lines(repo, HEAD, cited)[n - 1].strip()
        new_ref = re.sub(r":\d+$", ":%d" % n, ref)
        out.append((citing, at, ref, new_ref, "named", "-", code == holds, code[:70]))
    return out


def main():
    repo, mode = sys.argv[1], sys.argv[2]
    rows = derive(repo)
    bad = 0
    for citing, at, ref, new_ref, name, blamed, same, code in rows:
        bad += not same
        print("%s:%d  %s -> %s  frame %s (%s)  code the same: %s  | %s" % (citing, at, ref, new_ref, name, blamed, same, code))
    print("# %d references; %d whose code is not the same" % (len(rows), bad))
    if mode == "--apply":
        assert bad == 0
        by_file = {}
        for citing, at, ref, new_ref, *_ in rows:
            by_file.setdefault(citing, []).append((at, ref, new_ref))
        for citing, edits in by_file.items():
            path = os.path.join(repo, citing)
            text = open(path, encoding="utf-8").read().split("\n")
            for at, ref, new_ref in edits:
                assert text[at - 1].count(ref) >= 1
                # the reference as a token: not a longer number's prefix
                text[at - 1] = re.sub(re.escape(ref) + r"(?![0-9])", new_ref, text[at - 1], count=1)
            open(path, "w", encoding="utf-8").write("\n".join(text))


if __name__ == "__main__":
    main()
