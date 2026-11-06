"""The checkers against changed captures: what round 1's `pair.py` let
through, what `check.py` and the sweep (`sweep-view.py`, the narrowed
ledger) say about the same changes.

usage: python3 -I -B mutants.py <repo> <receipt-dir> <scratch-dir> <round-1-pair.py> <round-1-ledger.toml>

Each mutant copies the receipt's captures into <scratch-dir>, changes one row
of one capture, and runs the checkers on the copy. Nothing in the receipt or
the repository is written.
"""

import os
import re
import shutil
import subprocess
import sys

repo, receipt, scratch, old_pair, old_ledger = (os.path.realpath(p) for p in sys.argv[1:6])
UNIT = "p5-transcript-anchormid-40x120"


def capture(side, snap):
    prefix = "fx" if side == "fx" else "xfx"
    return os.path.join(scratch, "caps", side, UNIT, "%s-%s-%s.ansi" % (prefix, UNIT, snap))


def fresh():
    root = os.path.join(scratch, "caps")
    if os.path.exists(root):
        shutil.rmtree(root)
    for side in ("fx", "xfx"):
        shutil.copytree(os.path.join(receipt, side), os.path.join(root, side))


def change_row(side, snap, row, old, new):
    p = capture(side, snap)
    lines = open(p, encoding="utf-8").read().split("\n")
    assert old in lines[row - 1], (p, row, lines[row - 1])
    lines[row - 1] = lines[row - 1].replace(old, new, 1)
    open(p, "w", encoding="utf-8").write("\n".join(lines))


def run(argv, env=None):
    argv = [sys.executable if a == "python3" else a for a in argv]
    out = subprocess.run(argv, capture_output=True, text=True, env=env)
    return out.returncode, (out.stdout + out.stderr).strip().splitlines()


def tree(name, ledger_toml):
    """This repository's committed files (`git archive HEAD`), with
    `ledger_toml` as its parity ledger: what the ledger's source checks read."""
    dest = os.path.join(scratch, name)
    if not os.path.exists(dest):
        os.makedirs(dest)
        archive = subprocess.run(["git", "-C", repo, "archive", "HEAD"], capture_output=True, check=True).stdout
        subprocess.run(["tar", "-x", "-C", dest], input=archive, check=True)
        shutil.copy(ledger_toml, os.path.join(dest, "scripts", "fxcap", "parity-ledger.toml"))
    return dest


def old_checker(snap):
    env = {"PAGE_OFFSET": "2", "PATH": "/usr/bin:/bin"}
    # Round 1's pair.py read the ledger in the repo it was given; give it a
    # copy of this repo's fxcap scripts with round 1's ledger.
    old_repo = tree("round1-repo", old_ledger)
    code, lines = run(["python3", "-I", "-B", old_pair, old_repo, capture("fx", snap), capture("xfx", snap),
                       "session-record", "tool-schema"], env)
    return code, lines[-1]


def new_checker():
    code, lines = run(["python3", "-I", "-B", os.path.join(receipt, "check.py"), repo,
                       os.path.join(scratch, "caps"), "--only", "p5-transcript-anchormid"])
    fails = [l.strip() for l in lines if l.strip().startswith("FAIL")]
    return code, (fails[:2] if fails else lines[-1:])


def sweep(snap, ledger_toml):
    view_repo = tree("view-" + os.path.basename(os.path.dirname(ledger_toml)), ledger_toml)
    code, lines = run(["python3", "-I", "-B", os.path.join(receipt, "sweep-view.py"), view_repo,
                       os.path.join(scratch, "caps"), "%s/%s" % (UNIT, snap)])
    return code, [l for l in lines if l.strip()][-3:]


def last_digit(side, snap, row):
    """The row's last digit, one more (mod 10): the same shape, another line."""
    p = capture(side, snap)
    lines = open(p, encoding="utf-8").read().split("\n")
    line = lines[row - 1]
    # A digit the row prints: none inside a colour sequence.
    inside = set()
    for m in re.finditer(r"\x1b\[[0-9;]*m", line):
        inside.update(range(m.start(), m.end()))
    digits = [i for i, ch in enumerate(line) if ch.isdigit() and i not in inside]
    assert digits, line
    i = digits[-1]
    lines[row - 1] = line[:i] + str((int(line[i]) + 1) % 10) + line[i + 1 :]
    open(p, "w", encoding="utf-8").write("\n".join(lines))


# Each: a name, and the changes, as (side, snap, row, old, new) -- old None
# for "the row's last digit, one more". A content row of xfx's page is in
# two of xfx's windows (`full-pgup` row r is `full-pgup-up` row r + 3): a
# defect that moved it would show in both, so the "both" mutants change both.
MUTANTS = [
    ("none", []),
    ("xfx full-pgup row 1, the top edge, its last digit (that window only)",
     [("xfx", "full-pgup", 1, None, None)]),
    ("xfx full-pgup row 1, the top edge, its last digit, in both windows that show it",
     [("xfx", "full-pgup", 1, None, None), ("xfx", "full-pgup-up", 4, None, None)]),
    ("fx full-pgup row 36, the bottom edge: the header's `Tool` -> `Toil`",
     [("fx", "full-pgup", 36, "· Tool", "· Toil")]),
    ("xfx full-pgup row 12, mid-page, its last digit, in both windows that show it",
     [("xfx", "full-pgup", 12, None, None), ("xfx", "full-pgup-up", 15, None, None)]),
]

for name, changes in MUTANTS:
    fresh()
    for side, snap, row, old, new in changes:
        if old is None:
            last_digit(side, snap, row)
        else:
            change_row(side, snap, row, old, new)
    print("== mutant: %s" % name)
    code, last = old_checker("full-pgup")
    print("   round 1 pair.py (PAGE_OFFSET=2): exit %d  %s" % (code, last))
    code, lines = new_checker()
    print("   check.py:                        exit %d  %s" % (code, " | ".join(lines)))
    for label, toml in (("round 1 ledger", old_ledger), ("this ledger", os.path.join(repo, "scripts", "fxcap", "parity-ledger.toml"))):
        code, lines = sweep("full-pgup", toml)
        print("   sweep, %-14s:           exit %d  %s" % (label, code, " | ".join(l.strip() for l in lines)))
