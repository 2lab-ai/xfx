#!/usr/bin/env python3
"""The references refs_all.py and refs_short.py still flag at the head outside
the historical plans (`docs/superpowers/plans/`), each shown against `main`
(884497c0): the citing line at the head, and the line or lines the reference
names in `main`'s copy of the cited file. Each names other code on `main`
already -- stale before this train -- so the merges did not move what it is
about, and it is not re-pointed here.

    python3 -I -B stale.py <repo> <head>
"""
import subprocess
import sys

MAIN = "884497c0"
REFS = [
    # citing file, the text that cites, cited path, first, last, what the citation says the code is
    ("scripts/smoke-tui.sh", "src/tui/check.rs:1084", "src/tui/check.rs", 1084, 1084, "what the oracle licenses (train 4: stale since 9acf15d1)"),
    ("tests/tui.rs", "src/tui/shell.rs:2015", "src/tui/shell.rs", 2015, 2015, "`Reason::ExternalDamage`"),
    ("tests/tui.rs", "src/tui/shell.rs:2703", "src/tui/shell.rs", 2703, 2703, "`resize_pending || screen != geometry`"),
    ("tests/tui.rs", "tests/tui.rs:1191", "tests/tui.rs", 1191, 1191, "a case that clears the line"),
    ("scripts/smoke-tui.sh", "shell.rs:2597", "src/tui/shell.rs", 2597, 2597, "one `editor.insert()` call"),
    ("src/tui/event_loop.rs", "event_loop.rs:959-962", "src/tui/event_loop.rs", 959, 962,
     "the frame after a refused append not run (at 063fc66e, 2026-10-06: `if !commit_document(..)? { return Ok(()) }`)"),
    ("src/tui/event_loop.rs", "shell.rs:2706", "src/tui/shell.rs", 2706, 2706, "`InsertNewline`"),
    ("src/tui/shell.rs", "shell.rs:5816", "src/tui/shell.rs", 5816, 5816, "how the too-small assertions read it"),
    ("src/tui/wrap.rs", "check.rs:1689-1694", "src/tui/check.rs", 1689, 1694, "the check handing wrap a segmented ASCII run"),
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
    print(f"{len(REFS)} references, each naming other code on main")


main()
