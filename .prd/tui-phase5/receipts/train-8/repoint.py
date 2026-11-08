#!/usr/bin/env python3
"""Train 8's re-point: every `path:line` reference main's text holds whose
code merge 1 (GRID-ROW-COST) or merge 2 (FLAKE-MCPMODE) moved -- refs.py's,
refs_all.py's and refs_short.py's `main MOVED` rows -- set to where that code
is at the head, in the citing file. A reference is matched with the scans'
own boundaries (no digit, `-digit` or `,digit` after it, no `@`; a short one
with no path character before it), so `frame.rs:1617` is not the start of
`frame.rs:1617-1620`. Run once on the branch; refuses a reference it cannot
find exactly as many times as REPOINTS says.

    python3 -I -B repoint.py <checkout>
"""
import pathlib
import re
import sys

# citing file, reference as cited, as re-pointed, occurrences in the citing file
REPOINTS = [
    ("scripts/fxcap/parity-ledger.toml", "scripts/fxcap/capture.sh:315-317", "scripts/fxcap/capture.sh:317-319", 1),
    ("scripts/fxcap/parity-ledger.toml", "scripts/fxcap/capture.sh:521", "scripts/fxcap/capture.sh:538", 1),
    ("scripts/fxcap/parity-ledger.toml", "scripts/fxcap/capture.sh:952", "scripts/fxcap/capture.sh:973", 1),
    ("scripts/fxcap/parity-ledger.toml", "src/tui/frame.rs:2749", "src/tui/frame.rs:2757", 1),
    ("scripts/fxcap/parity-ledger.toml", "src/tui/full_transcript.rs:3005", "src/tui/full_transcript.rs:3010", 2),
    ("scripts/fxcap/parity-ledger.toml", "src/tui/full_transcript.rs:3056", "src/tui/full_transcript.rs:3061", 2),
    ("scripts/fxcap/parity-ledger.toml", "src/tui/grid.rs:933", "src/tui/grid.rs:1088", 1),
    ("scripts/fxcap/parity-ledger.toml", "src/tui/toolgroup.rs:922", "src/tui/toolgroup.rs:940", 2),
    ("scripts/fxcap/parity-ledger.toml", "src/tui/toolgroup.rs:999", "src/tui/toolgroup.rs:1017", 1),
    ("scripts/fxcap/parity-ledger.toml", "src/tui/toolgroup.rs:1113", "src/tui/toolgroup.rs:1131", 1),
    ("src/tui/frame.rs", "frame.rs:1063-1228", "frame.rs:1069-1234", None),
    ("src/tui/frame.rs", "frame.rs:1085", "frame.rs:1091", None),
    ("src/tui/frame.rs", "frame.rs:1456", "frame.rs:1462", None),
    ("src/tui/frame.rs", "frame.rs:1533-1585", "frame.rs:1539-1591", None),
    ("src/tui/frame.rs", "frame.rs:1587-1598", "frame.rs:1593-1604", None),
    ("src/tui/frame.rs", "frame.rs:1593-1616", "frame.rs:1599-1622", None),
    ("src/tui/frame.rs", "frame.rs:1617", "frame.rs:1623", None),
    ("src/tui/frame.rs", "frame.rs:1617-1620", "frame.rs:1623-1626", None),
    ("src/tui/frame.rs", "grid.rs:240-243", "grid.rs:242-245", None),
    ("src/tui/wrap.rs", "check.rs:1689-1694", "check.rs:1808-1813", None),
    ("src/tui/wrap.rs", "grid.rs:210-214", "grid.rs:212-216", None),
]


def pattern(ref):
    lead = r"\b" if "/" in ref else r"(?<![A-Za-z0-9_./-])"
    return re.compile(lead + re.escape(ref) + r"(?![\d@]|-\d|,\d)")


def main():
    checkout = pathlib.Path(sys.argv[1])
    for citing, old, new, count in REPOINTS:
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
