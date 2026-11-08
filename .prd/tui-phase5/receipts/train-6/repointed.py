#!/usr/bin/env python3
"""Train 6's re-pointed refs, each checked: the code the reference was written
about, in the frame of the side whose text cites it (`main` 690aa505 for the
references `main` holds; T-U4 5a7bb465 or CTRLO-WIDER-F 1824e868 for the ones
each added), against the code at the new line of the head.

    python3 -I -B repointed.py <repo> <head>

Each range: the text of `was` .. `was + n - 1` in the frame equals the text
of `now` .. at the head, and so do the two lines on either side (`ctx`, fewer
where a unit inserted lines right beside it; the row says how many).
"""
import subprocess
import sys

ROWS = [
    # who cites it, path, was, frame, now, n, ctx
    ("ledger ctrl-o-command-marks (source)", "src/tui/full_transcript.rs", 492, "690aa505", 495, 1, 2),
    # The line alone: CTRLO-WIDER rewrote `upstream_cells`' doc comment and
    # body around its signature (a count a display unit at a time), so the
    # lines either side differ; the signature is the same.
    ("ledger filler-modifier-count (source)", "src/tui/wrap.rs", 622, "690aa505", 656, 1, 0),
    ("ledger ctrl-o-drawn-wider-named, answer-modifier-sequence-wider (source)", "src/tui/full_transcript.rs", 2615, "1824e868", 2743, 1, 2),
    ("ledger answer-drawn-wider-named, answer-label-and-modifier-named (source)", "src/tui/full_transcript.rs", 2666, "1824e868", 2794, 1, 2),
    ("ledger answer-drawn-wider-named (source)", "src/tui/shell.rs", 12531, "1824e868", 12859, 1, 2),
    ("ledger session-record, transcript-page-offset, toolgroup-ctrlo-record-short, safe-page-edge, full-transcript-shift (xfx)", "src/tui/shell.rs", 37758, "690aa505", 37764, 1, 2),
    ("ledger cancel-page-edge (xfx)", "src/tui/shell.rs", 37424, "5a7bb465", 37764, 1, 2),
    ("ledger amendment-card-after-stop (xfx, why)", "src/tui/shell.rs", 9665, "5a7bb465", 9762, 1, 2),
    ("ledger ri-run-pairing (source)", "src/tui/wrap.rs", 593, "1824e868", 612, 12, 2),
    ("ledger resize-document-repaint (source)", "src/tui/shell.rs", 15273, "1824e868", 15607, 1, 2),
    ("capfile.py TIMERS, `warning_shown`", "src/tui/shell.rs", 14033, "690aa505", 14039, 26, 2),
    ("capfile.py TIMERS, the full transcript", "src/tui/shell.rs", 14051, "690aa505", 14057, 3, 2),
    ("event_loop.rs, the wrap into 40 + 40 and 10", "src/tui/wrap.rs", 213, "690aa505", 232, 9, 2),
]


def show(repo, commit, path):
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], check=True,
                          capture_output=True, text=True).stdout.split("\n")


def main():
    repo, head = sys.argv[1], sys.argv[2]
    ok = 0
    for who, path, was, frame, now, n, ctx in ROWS:
        a, b = show(repo, frame, path), show(repo, head, path)
        same = a[was - 1 - ctx:was - 1 + n + ctx] == b[now - 1 - ctx:now - 1 + n + ctx]
        ok += same
        span = lambda x: f"{x}" + (f"-{x + n - 1}" if n > 1 else "")
        print(f"{'SAME' if same else 'DIFFERENT'}  {path}:{span(was)}@{frame} -> :{span(now)} "
              f"(+-{ctx} lines)  [{who}]\n    first: {b[now - 1].strip()[:110]}")
    print(f"# {ok} of {len(ROWS)} the same code")


main()
