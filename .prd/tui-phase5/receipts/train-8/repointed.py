#!/usr/bin/env python3
"""Train 8's `path:line` numbers, each checked against its code: those the
re-point commit moved (the merges moved the code under them; repoint.py).
Each row: the code the reference was written about, in the frame of the
side whose text cites it (main 58c92ba2: both units left these references
as main has them), against the code at the number the head holds. Train
7's repointed.py with this train's rows.

    python3 -I -B repointed.py <repo> <head>

Train 6's check: the text of `was` .. `was + n - 1` in the frame equals the
text of `now` .. at the head, and so do `ctx` lines on either side. Train 7
adds a row whose cited line a unit rewrote since the frame: `ctx` 0, and the
two lines need only hold `same` (the call the reference names).
"""
import subprocess
import sys

ROWS = [
    # who cites it, path, was, frame, now, n, ctx, same (None: equal lines)
    # -- moved by merge 2 (FLAKE-MCPMODE: put_in_home and its comment, above them) --
    ("ledger (capture.sh's TMPDIR comment)", "scripts/fxcap/capture.sh", 315, "58c92ba2", 317, 3, 2, None),
    ("ledger (capture.sh's run directory)", "scripts/fxcap/capture.sh", 521, "58c92ba2", 538, 1, 2, None),
    ("ledger (capture.sh's remain-on-exit)", "scripts/fxcap/capture.sh", 952, "58c92ba2", 973, 1, 2, None),
    # -- moved by merge 1 (GRID-ROW-COST) --
    ("ledger (frame.rs)", "src/tui/frame.rs", 2749, "58c92ba2", 2757, 1, 2, None),
    ("ledger (full_transcript.rs, two entries)", "src/tui/full_transcript.rs", 3005, "58c92ba2", 3010, 1, 2, None),
    ("ledger (full_transcript.rs, two entries)", "src/tui/full_transcript.rs", 3056, "58c92ba2", 3061, 1, 2, None),
    ("ledger (grid.rs)", "src/tui/grid.rs", 933, "58c92ba2", 1088, 1, 2, None),
    ("ledger call-row-raw-argument, call-row-page-edge: T-U4's `spelled`", "src/tui/toolgroup.rs", 922, "58c92ba2", 940, 1, 2, None),
    ("ledger call-row-source-cut: `command_spelled`", "src/tui/toolgroup.rs", 999, "58c92ba2", 1017, 1, 2, None),
    ("ledger call-row-arabic-letter-mark: the U+061C arm", "src/tui/toolgroup.rs", 1113, "58c92ba2", 1131, 1, 2, None),
    ("frame.rs comment", "src/tui/frame.rs", 1063, "58c92ba2", 1069, 166, 2, None),
    ("frame.rs comment", "src/tui/frame.rs", 1085, "58c92ba2", 1091, 1, 2, None),
    ("frame.rs comment", "src/tui/frame.rs", 1456, "58c92ba2", 1462, 1, 2, None),
    ("frame.rs comment", "src/tui/frame.rs", 1533, "58c92ba2", 1539, 53, 2, None),
    ("frame.rs comment", "src/tui/frame.rs", 1587, "58c92ba2", 1593, 12, 2, None),
    ("frame.rs comment", "src/tui/frame.rs", 1593, "58c92ba2", 1599, 24, 2, None),
    ("frame.rs comment", "src/tui/frame.rs", 1617, "58c92ba2", 1623, 1, 2, None),
    ("frame.rs comment", "src/tui/frame.rs", 1617, "58c92ba2", 1623, 4, 2, None),
    ("frame.rs comment (grid.rs)", "src/tui/grid.rs", 240, "58c92ba2", 242, 4, 2, None),
    ("wrap.rs comment (check.rs; stale on main already, train 7's stale.txt)", "src/tui/check.rs", 1689, "58c92ba2", 1808, 6, 2, None),
    ("wrap.rs comment (grid.rs)", "src/tui/grid.rs", 210, "58c92ba2", 212, 5, 2, None),
]


def show(repo, commit, path):
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], check=True,
                          capture_output=True, text=True).stdout.split("\n")


def main():
    repo, head = sys.argv[1], sys.argv[2]
    ok = 0
    for who, path, was, frame, now, n, ctx, same in ROWS:
        at = head
        a, b = show(repo, frame, path), show(repo, at, path)
        if same is None:
            hit = a[was - 1 - ctx:was - 1 + n + ctx] == b[now - 1 - ctx:now - 1 + n + ctx]
        else:
            hit = same in a[was - 1] and same in b[now - 1]
        ok += hit
        span = lambda x: f"{x}" + (f"-{x + n - 1}" if n > 1 else "")
        how = f"+-{ctx} lines" if same is None else f"both hold {same!r}"
        print(f"{'SAME' if hit else 'DIFFERENT'}  {path}:{span(was)}@{frame} -> :{span(now)}@{at[:8]} "
              f"({how})  [{who}]\n    first: {b[now - 1].strip()[:110]}")
    print(f"# {ok} of {len(ROWS)} the same code")
    sys.exit(0 if ok == len(ROWS) else 1)


main()
