#!/usr/bin/env python3
"""Train 9's `path:line` numbers, each checked against its code: those the
re-point commits moved (merge 3, PASTE-AFTER-END, moved the code under them,
and merge 4 one of them again; repoint.py). Each row: the code the reference was written about, in the
frame of the side whose text cites it (main ccced1f5: every unit left these
references as main has them), against the code at the number the head
holds. Train 8's repointed.py with this train's rows.

    python3 -I -B repointed.py <repo> <head>

Train 6's check: the text of `was` .. `was + n - 1` in the frame equals the
text of `now` .. at the head, and so do `ctx` lines on either side.
"""

import subprocess
import sys

ROWS = [
    # who cites it, path, was, frame, now, n, ctx, same (None: equal lines)
    # -- moved by merge 3 (PASTE-AFTER-END: the paste refusal deleted above them, its tests added) --
    ("ledger (shell.rs: `send`)", "src/tui/shell.rs", 13112, "ccced1f5", 13094, 1, 2, None),
    ("ledger (shell.rs: `full_transcript::shown_event`)", "src/tui/shell.rs", 13423, "ccced1f5", 13405, 1, 2, None),
    ("ledger (shell.rs: `resize`)", "src/tui/shell.rs", 16321, "ccced1f5", 16303, 1, 2, None),
    ("ledger, six entries (shell.rs: `tools: {} advertised`; re-pointed again for merge 4)", "src/tui/shell.rs", 38615, "ccced1f5", 38716, 1, 2, None),
    ("capfile.py comment (shell.rs: `warning_shown`)", "src/tui/shell.rs", 14603, "ccced1f5", 14585, 26, 2, None),
    ("capfile.py comment, its bare continuation (shell.rs: `warning_shown`'s owner check; opus, review of a1a85ed6)", "src/tui/shell.rs", 14621, "ccced1f5", 14603, 3, 2, None),
    ("smoke-tui.sh comment (input.rs; names other code on main already, stale.txt)", "src/tui/input.rs", 702, "ccced1f5", 708, 1, 2, None),
    ("event_loop.rs comment (event_loop.rs; train 8's stale.txt)", "src/tui/event_loop.rs", 972, "ccced1f5", 1000, 4, 2, None),
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
