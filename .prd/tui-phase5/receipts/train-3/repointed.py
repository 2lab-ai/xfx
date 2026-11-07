#!/usr/bin/env python3
"""The ledger's re-pointed refs (b4e0d06e), each checked: the code the entry
was written about, in the frame it was written in, against the code at the
new line of the head.

    python3 -I -B repointed.py <repo> <head>

`was` is the line the ref named; `meant` is the line in the writer's frame
that holds the code the entry describes (the same as `was` unless the ref was
stale in its own frame); `now` is the head's line. The check: the text of
`meant` in the frame equals the text of `now` at the head, and so do the two
lines on either side of it.
"""
import subprocess
import sys

ROWS = [
    # entry(s), path, was, frame, meant, now
    ("submit-multi-card-whole (why)", "src/tui/shell.rs", 12267, "e51685d6", 12267, 12275),
    ("session-record, transcript-page-offset, safe-page-edge, full-transcript-shift",
     "src/tui/shell.rs", 37356, "e51685d6", 37440, 37458),
    ("toolgroup-ctrlo-record-short", "src/tui/shell.rs", 37272, "ec390074", 37374, 37458),
    ("cancelled-marker (xfx)", "src/tui/toolgroup.rs", 618, "97644085", 618, 711),
    ("call-row-raw-argument (source), call-row-page-edge (xfx)", "src/tui/toolgroup.rs", 657, "97644085", 657, 750),
    ("call-row-source-cut (source)", "src/tui/toolgroup.rs", 705, "97644085", 705, 798),
    ("call-row-arabic-letter-mark (source)", "src/tui/toolgroup.rs", 819, "97644085", 819, 912),
]


def lines(repo, commit, path):
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], check=True,
                          capture_output=True, text=True).stdout.splitlines()


def main():
    repo, head = sys.argv[1], sys.argv[2]
    bad = 0
    for entry, path, was, frame, meant, now in ROWS:
        old, new = lines(repo, frame, path), lines(repo, head, path)
        same = all(old[meant - 1 + k] == new[now - 1 + k] for k in (-2, -1, 0, 1, 2))
        bad += not same
        stale = "" if was == meant else f" (stale in its frame: {frame}:{was} is {old[was - 1].strip()[:40]!r})"
        print(f"{entry}\n  {path}:{was} -> :{now}; frame {frame}, meant :{meant}{stale}\n"
              f"  {'SAME' if same else 'DIFFERENT'} (5 lines): {new[now - 1].strip()[:90]!r}")
    print(f"{len(ROWS) - bad} of {len(ROWS)} the same code")
    sys.exit(1 if bad else 0)


main()
