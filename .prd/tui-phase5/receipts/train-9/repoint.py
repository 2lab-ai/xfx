#!/usr/bin/env python3
"""Train 9's re-point: every `path:line` reference main's text holds whose
code merge 3 (PASTE-AFTER-END) moved -- refs.py's, refs_all.py's and
refs_short.py's `main MOVED` rows -- set to where that code is at the head,
in the citing file; the historical plans under `docs/superpowers/plans/`
left, as train 8 left them (stale.txt). Two of the seven name other code on
`main` already (`smoke-tui.sh`'s `input.rs:702`, `event_loop.rs`'s
`event_loop.rs:972-975`): re-pointed by line, as train 8 re-pointed
`wrap.rs`'s `check.rs:1689-1694`, so each names what it named on `main`.
Train 8's repoint.py with this train's rows. Run once on the branch for
merge 3 (REPOINTS), and once more for merge 4 (`--merge-4`, REPOINTS_4:
HISTORY-NOTICE-STOP's 80 lines of tests in `shell.rs` moved the code under
the one of merge 3's re-points below them again); refuses a reference it
cannot find exactly as many times as the list says.

    python3 -I -B repoint.py <checkout> [--merge-4 | --bare]
"""

import pathlib
import re
import sys

# citing file, reference as cited, as re-pointed, occurrences in the citing file
REPOINTS = [
    ("scripts/fxcap/parity-ledger.toml", "src/tui/shell.rs:13112", "src/tui/shell.rs:13094", 1),
    ("scripts/fxcap/parity-ledger.toml", "src/tui/shell.rs:13423", "src/tui/shell.rs:13405", 1),
    ("scripts/fxcap/parity-ledger.toml", "src/tui/shell.rs:16321", "src/tui/shell.rs:16303", 1),
    ("scripts/fxcap/parity-ledger.toml", "src/tui/shell.rs:38615", "src/tui/shell.rs:38636", 6),
    ("scripts/fxcap/capfile.py", "src/tui/shell.rs:14603-14628", "src/tui/shell.rs:14585-14610", 1),
    ("scripts/smoke-tui.sh", "input.rs:702", "input.rs:708", 1),
    ("src/tui/event_loop.rs", "event_loop.rs:972-975", "event_loop.rs:1000-1003", 1),
]
REPOINTS_4 = [
    ("scripts/fxcap/parity-ledger.toml", "src/tui/shell.rs:38636", "src/tui/shell.rs:38716", 6),
]
# The bare continuation of merge 3's `capfile.py` re-point, which no scanner
# read (opus, review of a1a85ed6; refs_bare.py reads the form now): main's
# `:14621-14623`, the owner check in `warning_shown`, is `:14603-14605` at
# the head. Its own commit (`--bare`).
REPOINTS_BARE = [
    ("scripts/fxcap/capfile.py", "transcript at :14621-14623;", "transcript at :14603-14605;", 1),
]


def pattern(ref):
    lead = r"\b" if "/" in ref else r"(?<![A-Za-z0-9_./-])"
    return re.compile(lead + re.escape(ref) + r"(?![\d@]|-\d|,\d)")


def main():
    checkout = pathlib.Path(sys.argv[1])
    rows = {("--merge-4",): REPOINTS_4, ("--bare",): REPOINTS_BARE}.get(tuple(sys.argv[2:]), REPOINTS)
    for citing, old, new, count in rows:
        path = checkout / citing
        text = path.read_text()
        found = len(pattern(old).findall(text))
        if count is not None and found != count:
            sys.exit(f"{citing}: {old} found {found} times, not {count}")
        if found == 0:
            sys.exit(f"{citing}: {old} not found")
        path.write_text(pattern(old).sub(new, text))
        print(f"{citing}: {old} -> {new} ({found})")


main()
