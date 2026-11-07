"""One mutant per kind of claim `check.py` makes, each asserted: `check.py`
passes the unchanged captures and fails every mutant with the FAIL line it
names.

usage: python3 -I -B mutants.py <repo> <receipt-dir> <scratch-dir>

Each mutant copies the receipt's `fx/` and `xfx/` into <scratch-dir> with
this `check.py`, changes rows of one capture (keeping its rows), and runs the
checker there. Nothing in the receipt or the repository is written. Exit 0
only when every assertion holds.
"""

import os
import shutil
import subprocess
import sys

repo, receipt, scratch = (os.path.realpath(p) for p in sys.argv[1:4])


def capture(side, scenario, snap):
    prefix = "fx" if side == "fx" else "xfx"
    return os.path.join(scratch, "r", side, scenario + "-40x120", "%s-%s-40x120-%s.ansi" % (prefix, scenario, snap))


def edit(side, scenario, snap, change):
    p = capture(side, scenario, snap)
    lines = open(p, encoding="utf-8").read().split("\n")
    out = change(list(lines))
    assert len(out) == len(lines), (p, len(out), len(lines))
    open(p, "w", encoding="utf-8").write("\n".join(out))


def replace(row, old, new):
    def change(lines):
        assert old in lines[row - 1], (row, lines[row - 1])
        lines[row - 1] = lines[row - 1].replace(old, new, 1)
        return lines
    return change


def blank(row):
    def change(lines):
        lines[row - 1] = ""
        return lines
    return change


def swap(a, b):
    def change(lines):
        lines[a - 1], lines[b - 1] = lines[b - 1], lines[a - 1]
        return lines
    return change


MUTANTS = [
    ("T7: xfx's first frame clears the composer without the card (the card a frame late, as old fx)",
     [("xfx", "p5-transcript-submit", "frame1", blank(34))],
     "FAIL xfx frame 1: the card shows [], the claim is ['second prompt']"),
    ("T7: xfx's first frame keeps the draft in the composer",
     [("xfx", "p5-transcript-submit-multi", "frame1", replace(38, "┃", "┃ multi line three"))],
     "FAIL xfx frame 1: the composer row is '┃ multi line three' (the hint 'auto · model-a'), not an empty composer"),
    ("T7: the replayed wire is not the snap it must be",
     [("fx", "p5-transcript-submit", "wireend", replace(2, "Filler", "Fillet"))],
     "FAIL fx: the replayed wire is not the `second` snap: rows [2]"),
    ("T8: two of xfx's tall card rows change places in scrollback",
     [("xfx", "p5-transcript-tallfirst", "tall.full", swap(28, 29))],
     "FAIL xfx: the card's rows from row 3 are not in order, one after another"),
    ("T8: one of xfx's tall card rows is lost from scrollback",
     [("xfx", "p5-transcript-tallfirst", "tall.full", blank(30))],
     "FAIL xfx: 1 of 60 card rows not exactly once ([('┃ tall prompt body 27', 0)])"),
    # A raw ESC never reaches a capture as text: the terminal acts on it, so
    # the row loses what it was spelled as, and that is what fails.
    ("T10: xfx's Ctrl-O row hands the terminal the file's ESC[2J",
     [("xfx", "p5-transcript-diffctl", "full", replace(27, "\\u{001B}", "\x1b"))],
     "FAIL xfx row 27: 'beforecleared', xfx's spelling is 'before\\\\u{001B}[2Jcleared'"),
    ("T10: xfx's Ctrl-O row hands the terminal the file's bidirectional override",
     [("xfx", "p5-transcript-diffodd", "full", replace(27, "\\u{202E}", "\u202e"))],
     "FAIL xfx row 27: the terminal was handed ['U+202E'] as itself"),
    ("T10: xfx's review spells the C1 control in fx's lower case",
     [("xfx", "p5-transcript-diffask", "ask", replace(12, "\\u{009B}", "\\u{009b}"))],
     "FAIL xfx row 12: 'second \\\\u{009b}line', xfx's spelling is 'second \\\\u{009B}line'"),
    ("T10: an inline transcript draws a diff row",
     [("xfx", "p5-transcript-diffodd", "edited", replace(10, "", "  │ 1 + FIRST LINE"))],
     "FAIL xfx: diff rows [10]; 1 `└ Edited` rows"),
]


def fresh():
    root = os.path.join(scratch, "r")
    if os.path.exists(root):
        shutil.rmtree(root)
    os.makedirs(root)
    for side in ("fx", "xfx"):
        shutil.copytree(os.path.join(receipt, side), os.path.join(root, side))
    shutil.copy(os.path.join(receipt, "check.py"), os.path.join(root, "check.py"))


def checked():
    root = os.path.join(scratch, "r")
    out = subprocess.run([sys.executable, "-I", "-B", os.path.join(root, "check.py"), repo, root, "xfx"],
                         capture_output=True, text=True)
    lines = (out.stdout + out.stderr).strip().splitlines()
    fails = [l.strip() for l in lines if l.startswith("       FAIL ")]
    summary = next((l for l in lines if l.endswith(" fail")), lines[-1:] and lines[-1])
    return out.returncode, fails, summary


broken = []
fresh()
code, fails, summary = checked()
print("== unchanged captures: exit %d  %s" % (code, summary))
if code != 0:
    broken.append("check.py fails the unchanged captures")
for name, changes, expected in MUTANTS:
    fresh()
    for side, scenario, snap, change in changes:
        edit(side, scenario, snap, change)
    code, fails, summary = checked()
    print("== mutant: %s\n   exit %d  %s\n   %s" % (name, code, summary, " | ".join(fails[:2])))
    if code != 1 or expected not in fails:
        broken.append("%r is not failed with %r (exit %d, %s)" % (name, expected, code, fails))
print("%d mutants; %s" % (len(MUTANTS), "every assertion held" if not broken else "%d assertions broke:" % len(broken)))
for line in broken:
    print("   BROKEN " + line)
sys.exit(1 if broken else 0)
