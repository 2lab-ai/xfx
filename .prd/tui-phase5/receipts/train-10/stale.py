#!/usr/bin/env python3
"""The references the scans flag at train 10's head that this train did not
re-point, or re-pointed by line though they name other code on `main`
(6bb9fd90) already -- each shown against `main`: the citing line at the head,
and the lines the reference names in `main`'s copy of the cited file.
Train 9's stale.py with `main` moved to 6bb9fd90 and this train's rows.

- the historical plans under `docs/superpowers/plans/`: the merges move code
  under some of their references; each was written about its plan's day, and
  trains 8 and 9 left them for REF-LINT -- left so here (`refs-all.txt`).
  One shown, as train 9 showed it.
- the ledger's bare `:1658` after `src/tui/shell.rs:10277` (the
  `trailer_rows` cause, for `queue_card`): AT-HEIGHT wrote the line, and in
  its frame the number does not move, so it is not re-pointed; but neither
  `main`'s line 1658 nor the head's is `queue_card`, which is at `main`'s
  1831 and the head's 1840 -- stale before this train (written by train 7's
  c468f5e1).
- `event_loop.rs`'s `event_loop.rs:1005-1008` (train 8's stale.txt, re-pointed
  by line by train 9 and again here) and `frame.rs:1615-1628` (train 8's
  stale.txt; `CHANGED` for the units whose frames are older than
  GRID-ROW-COST).

    python3 -I -B stale.py <repo> <head>
"""
import subprocess
import sys

MAIN = "6bb9fd90"
REFS = [
    # citing file, the text that cites (at the head), cited path, first, last (on main), what the citation says the code is
    ("docs/superpowers/plans/2026-08-24-provider-frame.md", "tests/tool_loop.rs:1266", "tests/tool_loop.rs",
     1266, 1266, "a fake-provider `Completion { .. }` literal (the plan of 2026-08-24)"),
    ("scripts/fxcap/parity-ledger.toml", "each card through `queue_card`, :1658", "src/tui/shell.rs", 1658, 1658,
     "`queue_card` (on main at :1831, at the head at :1840)"),
    ("src/tui/event_loop.rs", "event_loop.rs:1005-1008", "src/tui/event_loop.rs", 1000, 1003,
     "the frame after a refused append not run (main's `:1000-1003`, re-pointed by line; train 8's stale.txt)"),
    ("src/tui/event_loop.rs", "frame.rs:1615-1628", "src/tui/frame.rs", 1615, 1628,
     "the escape `place()` writes once per document row (train 8's stale.txt)"),
]


def git(repo, *args):
    return subprocess.run(["git", "-C", repo, *args], check=True, capture_output=True, text=True).stdout


def main():
    repo, head = sys.argv[1], sys.argv[2]
    for citing, text, path, a, z, says in REFS:
        hit = [l for l in git(repo, "grep", "-n", "-F", text, head, "--", citing).splitlines()]
        cited = git(repo, "show", f"{MAIN}:{path}").splitlines()[a - 1:z]
        print(f"{citing} cites {text} for {says}")
        for line in hit:
            body = line.split(":", 2)[2]
            at = body.find(text)
            print(f"  at the head: ...{body[max(0, at - 60):at + 60].strip()}...")
        for k, line in enumerate(cited):
            print(f"  main {path}:{a + k}: {line.strip()[:110]}")
    print(f"{len(REFS)} references, each naming other code on main, or rewritten under before this train")
    q = [i + 1 for i, l in enumerate(git(repo, "show", f"{head}:src/tui/shell.rs").splitlines()) if l.startswith("fn queue_card(")]
    m = [i + 1 for i, l in enumerate(git(repo, "show", f"{MAIN}:src/tui/shell.rs").splitlines()) if l.startswith("fn queue_card(")]
    print(f"# fn queue_card: main shell.rs:{m}, head shell.rs:{q}")


main()
