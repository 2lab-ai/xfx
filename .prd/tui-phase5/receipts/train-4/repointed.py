#!/usr/bin/env python3
"""Train 4's re-pointed refs, each checked: the code the reference was written
about, in the frame it was written in, against the code at the new line of
the head.

    python3 -I -B repointed.py <repo> <head>

`was` is the line the reference named and `frame` the commit whose copy of
the cited file it was written against; `now` is the head's line. The check:
the text of `was` .. `was + n - 1` in the frame equals the text of `now` ..
at the head, and so do the two lines on either side.
"""
import subprocess
import sys

ROWS = [
    # who cites it, path, was, frame, now, n (lines in the range)
    ("call-row-raw-argument (source), call-row-page-edge (xfx)", "src/tui/toolgroup.rs", 750, "e6cde019", 749, 1),
    ("call-row-source-cut (source)", "src/tui/toolgroup.rs", 798, "e6cde019", 797, 1),
    ("call-row-arabic-letter-mark (source)", "src/tui/toolgroup.rs", 912, "e6cde019", 911, 1),
    ("cancelled-marker (xfx)", "src/tui/toolgroup.rs", 711, "e6cde019", 710, 1),
    ("submit-multi-card-whole (why; `send`)", "src/tui/shell.rs", 12275, "e6cde019", 12272, 1),
    ("session-record, transcript-page-offset, toolgroup-ctrlo-record-short, safe-page-edge, full-transcript-shift",
     "src/tui/shell.rs", 37458, "e6cde019", 37424, 1),
    ("run-directory (source)", "scripts/fxcap/capture.sh", 430, "d56ab017", 462, 1),
    ("capture-tmpdir (source)", "scripts/fxcap/capture.sh", 266, "d56ab017", 291, 3),
    ("tmux-dead-pane (source)", "scripts/fxcap/capture.sh", 794, "d56ab017", 878, 1),
    ("capfile.py TIMERS, `warning_shown`", "src/tui/shell.rs", 13708, "31a6ef79", 13705, 26),
    ("capfile.py TIMERS, the full transcript", "src/tui/shell.rs", 13726, "31a6ef79", 13723, 3),
]


def lines(repo, commit, path):
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], check=True,
                          capture_output=True, text=True).stdout.splitlines()


def main():
    repo, head = sys.argv[1], sys.argv[2]
    bad = 0
    for who, path, was, frame, now, n in ROWS:
        old, new = lines(repo, frame, path), lines(repo, head, path)
        same = all(old[was - 1 + k] == new[now - 1 + k] for k in range(-2, n + 2))
        bad += not same
        span = lambda a: f"{a}" if n == 1 else f"{a}-{a + n - 1}"
        print(f"{who}\n  {path}:{span(was)} ({frame}) -> :{span(now)}\n"
              f"  {'SAME' if same else 'DIFFERENT'} ({n + 4} lines): {new[now - 1].strip()[:90]!r}")
    print(f"{len(ROWS) - bad} of {len(ROWS)} the same code")
    sys.exit(1 if bad else 0)


main()
