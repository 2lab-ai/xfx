#!/usr/bin/env python3
"""The references the scans flag at train 11's head that this train did not
re-point, or re-pointed by line though they name other code on `main`
(32afd307) already -- each shown against `main`: the citing line at the head,
and the lines the reference names in `main`'s copy of the cited file.
Train 10's stale.py with `main` moved to 32afd307 and this train's rows.

- the historical plans under `docs/superpowers/plans/`: left for REF-LINT,
  as trains 8 to 10 left them. One shown, as trains 9 and 10 showed it.
- the ledger's bare `:1658` after `src/tui/shell.rs:10277` (the
  `trailer_rows` cause, for `queue_card`), re-pointed by line here to
  `:1747`: neither `main`'s 1658 nor the head's 1747 is `queue_card`,
  which is at `main`'s 1840 and the head's 1929 -- stale since train 7's
  c468f5e1 (train 10's stale.txt).
- `event_loop.rs`'s `event_loop.rs:1005-1008`, re-pointed by line to
  `:1046-1049` (train 8's stale.txt, trains 9 and 10 re-pointed it by line),
  and `frame.rs:1615-1628` (train 8's stale.txt; `CHANGED` for the units
  whose frames are older than GRID-ROW-COST).

    python3 -I -B stale.py <repo> <head>
"""
import subprocess
import sys

MAIN = "32afd307"
REFS = [
    # citing file, the text that cites (at the head), cited path, first, last (on main), what the citation says the code is
    ("docs/superpowers/plans/2026-08-24-provider-frame.md", "tests/tool_loop.rs:1266", "tests/tool_loop.rs",
     1266, 1266, "a fake-provider `Completion { .. }` literal (the plan of 2026-08-24)"),
    ("scripts/fxcap/parity-ledger.toml", "each card through `queue_card`, :1747", "src/tui/shell.rs", 1658, 1658,
     "`queue_card` (on main at :1840, at the head at :1929; main's text says :1658)"),
    ("src/tui/event_loop.rs", "event_loop.rs:1046-1049", "src/tui/event_loop.rs", 1005, 1008,
     "the frame after a refused append not run (main's `:1005-1008`, re-pointed by line; train 8's stale.txt)"),
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
