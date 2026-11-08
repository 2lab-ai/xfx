#!/usr/bin/env python3
"""Train 5's re-pointed refs, each checked: the code the reference was written
about, in the frame it was written in (`main` 884497c0), against the code at
the new line of the head.

    python3 -I -B repointed.py <repo> <head>

ROWS moved whole: the text of `was` .. `was + n - 1` in the frame equals the
text of `now` .. at the head, and so do the two lines on either side.

A `docs/parity.md` row is checked alone, with no lines either side: a row is
cited by its line, and the rows above it are the ones the merge inserted.

GROWN are the two ranges L-U1+U2 wrote into (`src/tui/term.rs`: a paragraph
of doc comment, and the program status clear inside each restore literal).
Their first line and the two either side of it are the same text in the frame
and at the head; their last line and the one above it are the same text,
once the unit's clear (`ESC ] 7501 ; state=clear ESC \\`) is taken out of
the head's where it has one.
"""
import subprocess
import sys

ROWS = [
    # who cites it, path, was, frame, now, n (lines in the range)
    ("parity-ledger.toml inline-code divergence (source), the `colored and hyperlinked TTY output` row",
     "docs/parity.md", 241, "884497c0", 243, 1),
    ("parity-ledger.toml banner normalization (why, source)", "src/tui/shell.rs", 184, "884497c0", 185, 1),
    ("parity-ledger.toml :849 (xfx)", "src/tui/shell.rs", 1583, "884497c0", 1644, 1),
    ("submit-multi-card-whole (why; `send`)", "src/tui/shell.rs", 12272, "884497c0", 12600, 1),
    ("session-record, transcript-page-offset, toolgroup-ctrlo-record-short, safe-page-edge, full-transcript-shift (xfx)",
     "src/tui/shell.rs", 37424, "884497c0", 37758, 1),
    ("capfile.py TIMERS, `warning_shown`", "src/tui/shell.rs", 13705, "884497c0", 14033, 26),
    ("capfile.py TIMERS, the full transcript", "src/tui/shell.rs", 13723, "884497c0", 14051, 3),
]

CLEAR = "\\x1b]7501;state=clear\\x1b\\\\"
GROWN = [
    # who cites it, path, was (first, last), frame, now (first, last)
    ("smoke-tui.sh, the mode set and the restores in order", "src/tui/term.rs", (56, 90), "884497c0", (56, 101)),
    ("smoke-tui.sh, every restore opens with ?2026l", "src/tui/term.rs", (74, 98), "884497c0", (74, 109)),
]


def lines(repo, commit, path):
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], check=True,
                          capture_output=True, text=True).stdout.splitlines()


def main():
    repo, head = sys.argv[1], sys.argv[2]
    bad = 0
    for who, path, was, frame, now, n in ROWS:
        old, new = lines(repo, frame, path), lines(repo, head, path)
        around = 0 if path == "docs/parity.md" else 2
        same = all(old[was - 1 + k] == new[now - 1 + k] for k in range(-around, n + around))
        bad += not same
        span = lambda a: f"{a}" if n == 1 else f"{a}-{a + n - 1}"
        print(f"{who}\n  {path}:{span(was)} ({frame}) -> :{span(now)}\n"
              f"  {'SAME' if same else 'DIFFERENT'} ({n + 2 * around} lines): {new[now - 1].strip()[:90]!r}")
    for who, path, (a, z), frame, (a2, z2) in GROWN:
        old, new = lines(repo, frame, path), lines(repo, head, path)
        first = all(old[a - 1 + k] == new[a2 - 1 + k] for k in range(-2, 3))
        tail = all(old[z - 1 - k] == new[z2 - 1 - k].replace(CLEAR, "", 1) for k in (0, 1))
        ok = first and tail
        bad += not ok
        print(f"{who}\n  {path}:{a}-{z} ({frame}) -> :{a2}-{z2}\n"
              f"  first line and two either side {'SAME' if first else 'DIFFERENT'}: {new[a2 - 1].strip()[:70]!r}\n"
              f"  last line and the one above it {'SAME' if tail else 'DIFFERENT'} (the clear aside): "
              f"{new[z2 - 2].strip()[:50]!r} / {new[z2 - 1].strip()[:60]!r}")
    total = len(ROWS) + len(GROWN)
    print(f"{total - bad} of {total} the same code")
    sys.exit(1 if bad else 0)


main()
