#!/usr/bin/env python3
"""The references that name other code on `main` (58c92ba2) already -- stale
before this train -- each shown against `main`: the citing line at the head,
and the line or lines the reference names in `main`'s copy of the cited
file. Train 7's stale.py with `main` moved to 58c92ba2.

Train 7's eleven (its stale.txt), one of them -- `wrap.rs`'s `check.rs:1689-1694`
-- re-pointed by this train's re-point commit to `:1808-1813`, where the same
lines of main are at the head (GRID-ROW-COST moved them), so still naming
what it named on main; and one more this train's scans flag:
`event_loop.rs`'s `frame.rs:1615-1628`, for the escape `place()` writes, which
names the reuse comment of `render_append_recorded` on main (`place` is
`frame.rs:2749` there), and whose lines GRID-ROW-COST rewrote (its comment
now names `Grid::row_holds`), so it cannot be mapped (refs_short.py:
`CHANGED`). Left for REF-LINT, as train 7 left its eleven.

    python3 -I -B stale.py <repo> <head>
"""
import subprocess
import sys

MAIN = "58c92ba2"
REFS = [
    # citing file, the text that cites, cited path, first, last, what the citation says the code is
    ("scripts/smoke-tui.sh", "src/tui/check.rs:1084", "src/tui/check.rs", 1084, 1084, "what the oracle licenses (train 4: stale since 9acf15d1)"),
    ("tests/tui.rs", "src/tui/shell.rs:2015", "src/tui/shell.rs", 2015, 2015, "`Reason::ExternalDamage`"),
    ("tests/tui.rs", "src/tui/shell.rs:2703", "src/tui/shell.rs", 2703, 2703, "`resize_pending || screen != geometry`"),
    ("tests/tui.rs", "tests/tui.rs:1191", "tests/tui.rs", 1191, 1191, "a case that clears the line"),
    ("scripts/smoke-tui.sh", "shell.rs:2597", "src/tui/shell.rs", 2597, 2597, "one `editor.insert()` call"),
    ("src/tui/event_loop.rs", "event_loop.rs:972-975", "src/tui/event_loop.rs", 959, 962,
     "the frame after a refused append not run (at 063fc66e, 2026-10-06: `if !commit_document(..)? { return Ok(()) }`); "
     "STOP-NEXT-2's re-point of main's `:959-962`, the head's lines below"),
    ("src/tui/event_loop.rs", "shell.rs:2706", "src/tui/shell.rs", 2706, 2706, "`InsertNewline`"),
    ("src/tui/shell.rs", "shell.rs:5816", "src/tui/shell.rs", 5816, 5816, "how the too-small assertions read it"),
    ("src/tui/wrap.rs", "check.rs:1808-1813", "src/tui/check.rs", 1689, 1694, "the check handing wrap a segmented ASCII run (main's `:1689-1694`, re-pointed by line)"),
    # Train 6: two more, flagged because T-U4 moved main's lines under them;
    # each names other code on main (`MAX_TAIL_ROWS` is main's line 347).
    ("src/tui/event_loop.rs", "transcript.rs:136", "src/tui/transcript.rs", 136, 136, "`MAX_TAIL_ROWS` (256)"),
    ("src/tui/event_loop.rs", "transcript.rs:505-531", "src/tui/transcript.rs", 505, 531, "the transcript adopting only on `Landed::All`"),
    # Train 8: GRID-ROW-COST rewrote lines inside this one's range.
    ("src/tui/event_loop.rs", "frame.rs:1615-1628", "src/tui/frame.rs", 1615, 1628, "the escape `place()` writes once per document row"),
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
            print(f"  at the head: {line.split(':', 2)[2].strip()[:110]}")
        for k, line in enumerate(cited):
            print(f"  main {path}:{a + k}: {line.strip()[:110]}")
        if text == "event_loop.rs:972-975":
            for k, line in enumerate(git(repo, "show", f"{head}:{path}").splitlines()[971:975]):
                print(f"  head {path}:{972 + k}: {line.strip()[:110]}")
    print(f"{len(REFS)} references, each naming other code on main")


main()
