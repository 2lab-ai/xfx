#!/usr/bin/env python3
"""Train 7's `path:line` numbers, each checked against its code: those the
re-point commit moved (the merges moved the code under them), and those the
merges themselves set where both sides had re-pointed one reference to
their own branch's line (merge 3's and merge 4's conflicts). Each row: the
code the reference was written about, in the frame of the side whose text
cites it, against the code at the number the head holds.

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
    # -- moved by the merges, re-pointed by the re-point commit --
    ("ledger (capture.sh's TMPDIR comment)", "scripts/fxcap/capture.sh", 291, "fdc5ac0b", 315, 3, 2, None),
    ("ledger (capture.sh's run directory)", "scripts/fxcap/capture.sh", 462, "fdc5ac0b", 521, 1, 2, None),
    ("ledger (capture.sh's remain-on-exit)", "scripts/fxcap/capture.sh", 878, "fdc5ac0b", 952, 1, 2, None),
    # STOP-NEXT rewrote the call (`input` became `estimate`) and the lines
    # around it; the reference names `tally.begin`.
    ("ledger model-parked's cause (why: `tally.begin`)", "src/tui/shell.rs", 4131, "66854557", 4333, 1, 0,
     "self.tally.begin(Instant::now(), "),
    ("ledger amendment-card-after-stop (xfx, why)", "src/tui/shell.rs", 9762, "fdc5ac0b", 10194, 1, 2, None),
    ("ledger answer-drawn-wider-named (source)", "src/tui/shell.rs", 12859, "fdc5ac0b", 13423, 1, 2, None),
    ("ledger resize-document-repaint (source)", "src/tui/shell.rs", 15607, "fdc5ac0b", 16321, 1, 2, None),
    ("ledger cancel-page-edge (xfx)", "src/tui/shell.rs", 37764, "fdc5ac0b", 38615, 1, 2, None),
    ("shell.rs comment (STOP-NEXT-2's `TurnEnded`, `worker.rs:3846`)", "src/tui/worker.rs", 3846, "f8d625c3", 3867, 1, 2, None),
    # -- set by merge 3 (50cabfa6), then moved by merge 4 --
    ("merge 3: ledger session-record and four more (xfx), at merge 3", "src/tui/shell.rs", 37758, "690aa505", 38014, 1, 2, None),
    # -- set by merge 4 (c080f5a5), at the head --
    ("merge 4: ledger session-record, transcript-page-offset, toolgroup-ctrlo-record-short, safe-page-edge, full-transcript-shift (xfx)",
     "src/tui/shell.rs", 37758, "690aa505", 38615, 1, 2, None),
    ("merge 4: ledger submit-multi-card-whole (why: `fn send`)", "src/tui/shell.rs", 12600, "690aa505", 13112, 1, 2, None),
    ("merge 4: ledger stats-bytes (xfx)", "src/tui/shell.rs", 1644, "690aa505", 1756, 1, 2, None),
    # T-U4-F's own re-point of T-U4's `spelled` (889 on train 6), 914 on its branch.
    ("merge 4: ledger call-row-raw-argument (source), call-row-page-edge (xfx): T-U4's `spelled`",
     "src/tui/toolgroup.rs", 914, "a8c7a87d", 922, 1, 2, None),
    ("merge 4: ledger call-row-source-cut (source): `command_spelled`", "src/tui/toolgroup.rs", 797, "690aa505", 999, 1, 2, None),
    ("merge 4: ledger call-row-arabic-letter-mark (source): the U+061C arm", "src/tui/toolgroup.rs", 911, "690aa505", 1113, 1, 2, None),
    ("merge 4: parity 'terminal program status (OSC 7501)': the end sent", "src/tui/worker.rs", 3835, "690aa505", 3867, 1, 2, None),
    ("merge 4: parity 'terminal program status (OSC 7501)': the place given back", "src/tui/worker.rs", 3524, "690aa505", 3550, 1, 2, None),
    ("merge 4: capfile.py TIMERS, `warning_shown`", "src/tui/shell.rs", 14033, "690aa505", 14603, 26, 2, None),
    ("merge 4: capfile.py TIMERS, the full transcript", "src/tui/shell.rs", 14051, "690aa505", 14621, 3, 2, None),
]


def show(repo, commit, path):
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], check=True,
                          capture_output=True, text=True).stdout.split("\n")


def main():
    repo, head = sys.argv[1], sys.argv[2]
    merge3 = "50cabfa6"
    ok = 0
    for who, path, was, frame, now, n, ctx, same in ROWS:
        at = merge3 if who.startswith("merge 3:") else head
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
