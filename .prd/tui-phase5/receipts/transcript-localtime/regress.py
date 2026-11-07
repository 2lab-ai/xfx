"""RED (the base) against GREEN (this branch) for every snap of every
p5-transcript capture: cell by cell (text and attributes) after the parity
ledger's normalizations. A row may differ only as an entry header whose zone
changed -- `  <clock> UTC · <Kind>` on RED, the same with another plain zone
on GREEN -- and is then counted, not failed.

usage: python3 -I -B regress.py <repo> <red-dir> <green-dir>

Each dir holds one sub-directory per capture (`p5-transcript-tz-40x120/`).
Prints one line per snap; exit 0 when no row differs otherwise.
"""

import os
import re
import sys

repo = os.path.realpath(sys.argv[1])
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))

import cells  # noqa: E402
import ledger as ledgermod  # noqa: E402

red_dir, green_dir = sys.argv[2], sys.argv[3]
rules = ledgermod.rules_for(ledgermod.load(os.path.join(repo, "scripts", "fxcap", "parity-ledger.toml"), repo), "xfx")
HEADER = re.compile(r"^(  X{23} )([A-Za-z0-9+-]{1,10})( · \w+)$")


def load(path):
    return [cells.normalize_row(r, rules) for r in cells.parse(open(path, encoding="utf-8").read())]


def as_utc(row):
    """The row with a header's zone read back as UTC, its cells re-measured."""
    text = cells.text_of(row)
    m = HEADER.match(text)
    if not m:
        return None
    shown = {col: attrs for col, _, attrs in row}
    first = shown.get(2, ())
    written = m.group(1) + "UTC" + m.group(3)
    return [(i, ch, first) for i, ch in enumerate(written)], m.group(2)


bad = 0
total = 0
for capture in sorted(os.listdir(green_dir)):
    for name in sorted(os.listdir(os.path.join(green_dir, capture))):
        if not name.endswith(".ansi"):
            continue
        total += 1
        red, green = load(os.path.join(red_dir, capture, name)), load(os.path.join(green_dir, capture, name))
        zones, other = {}, []
        for i in range(max(len(red), len(green))):
            a = red[i] if i < len(red) else []
            b = green[i] if i < len(green) else []
            if a == b:
                continue
            swapped = as_utc(b)
            ra = as_utc(a)
            if swapped and ra and ra[1] == "UTC" and swapped[1] != "UTC" and [(c, ch) for c, ch, _ in ra[0]] == [(c, ch) for c, ch, _ in swapped[0]] and {x for _, _, x in a} == {x for _, _, x in b}:
                zones[swapped[1]] = zones.get(swapped[1], 0) + 1
                continue
            other.append(i + 1)
        snap = name[len("xfx-"):-len(".ansi")]
        if other:
            bad += 1
            print("%-48s DIFFERS rows %s" % (snap, other))
            for row in other:
                for side, rows in (("red", red), ("green", green)):
                    print("    %3d %-5s %s" % (row, side, cells.text_of(rows[row - 1]) if row <= len(rows) else "(none)"))
        elif zones:
            print("%-48s identical but the zone: %s" % (snap, ", ".join("%d header(s) UTC -> %s" % (n, z) for z, n in sorted(zones.items()))))
        else:
            print("%-48s identical" % snap)
print("%d snaps, %d with a difference other than a header's zone" % (total, bad))
sys.exit(1 if bad or total == 0 else 0)
