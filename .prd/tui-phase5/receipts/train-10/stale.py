#!/usr/bin/env python3
"""The references the widened scans flag at train 9's head that this train
did not re-point, and the two it re-pointed by line that name other code on
`main` (ccced1f5) already -- each shown against `main`: the citing line at
the head, and the line or lines the reference names in `main`'s copy of the
cited file. Train 8's stale.py with `main` moved to ccced1f5 and this
train's rows.

- the historical plans under `docs/superpowers/plans/`: QUESTIONS-20 and
  PASTE-AFTER-END move code under 38 of their references (`tests/tool_loop.rs`,
  `approval.rs`, `approval_screen.rs`, `event_loop.rs`, `input.rs`; the
  scans' lists in `refs-all.txt`), each written about the code of its plan's
  day. Train 8 left these plans for REF-LINT: not re-pointed here either.
  One shown: `2026-08-24-provider-frame.md`'s `tests/tool_loop.rs:1266`.
- `smoke-tui.sh`'s `input.rs:702`, for `Action::Undo`: on `main` that line is
  a doc comment of the paste body; re-pointed by line (`:708`), so it names
  what it named on `main`.
- `event_loop.rs`'s `event_loop.rs:972-975`, for "the frame that would have
  followed it must not run": train 8's stale.txt; re-pointed by line
  (`:1000-1003`).
- `event_loop.rs`'s `frame.rs:1615-1628`: train 8's stale.txt; flagged again
  (`CHANGED`) because the units' frames are older than GRID-ROW-COST, which
  rewrote the lines; `main` and the head have the same lines.

    python3 -I -B stale.py <repo> <head>
"""
import subprocess
import sys

MAIN = "ccced1f5"
REFS = [
    # citing file, the text that cites (at the head), cited path, first, last (on main), what the citation says the code is
    ("docs/superpowers/plans/2026-08-24-provider-frame.md", "tests/tool_loop.rs:1266", "tests/tool_loop.rs",
     1266, 1266, "a fake-provider `Completion { .. }` literal (the plan of 2026-08-24)"),
    ("scripts/smoke-tui.sh", "input.rs:708", "src/tui/input.rs", 702, 702,
     "`Action::Undo` (main's `input.rs:702`, re-pointed by line)"),
    ("src/tui/event_loop.rs", "event_loop.rs:1000-1003", "src/tui/event_loop.rs", 972, 975,
     "the frame after a refused append not run (main's `:972-975`, re-pointed by line; train 8's stale.txt)"),
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


main()
