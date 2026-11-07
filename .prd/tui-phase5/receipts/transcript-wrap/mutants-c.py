"""Round 3 (T-U1U2c): one mutant per assertion `check.py` gained, run through
the `check.py` round 2 shipped and through this one.

usage: python3 -I -B mutants-c.py <repo> <receipt-dir> <scratch-dir> <round-2-check.py>

Each mutant copies the receipt's captures into <scratch-dir>, changes rows of
one or two captures (keeping each screen's 40 rows and its footer where it
was), and runs both checkers on the scenario. Nothing in the receipt or the
repository is written.
"""

import os
import shutil
import subprocess
import sys

repo, receipt, scratch, old_check = (os.path.realpath(p) for p in sys.argv[1:5])


def capture(side, unit, snap):
    prefix = "fx" if side == "fx" else "xfx"
    return os.path.join(scratch, "caps", side, unit, "%s-%s-%s.ansi" % (prefix, unit, snap))


def edit(side, unit, snap, change):
    p = capture(side, unit, snap)
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


def insert(row, text, drop):
    """`text` put in as row `row`, and row `drop` (counted before the
    insertion, inside the record) taken out, so the footer stays put."""
    def change(lines):
        del lines[drop - 1]
        lines.insert(row - 1, text)
        return lines
    return change


def drop_then_pad(row, pad_at):
    """Row `row` taken out, and a blank row put in at `pad_at` (counted after
    the removal), so the footer stays put."""
    def change(lines):
        del lines[row - 1]
        lines.insert(pad_at - 1, "")
        return lines
    return change


def chain(*changes):
    def change(lines):
        for c in changes:
            lines = c(lines)
        return lines
    return change


R = "p5-transcript-records-40x120"
T = "p5-transcript-tools-40x120"
Q = "p5-transcript-question-40x120"
A = "p5-transcript-anchormid-40x120"
MUTANTS = [
    ("none", "p5-transcript-records", []),
    ("records: xfx's tool list loses its closing `)`, and a row after it and one far below are changed "
     "(the tool list must close; gpt6-b's MUST)", "p5-transcript-records",
     [("xfx", R, "full-records", chain(replace(3, "+5 more)", "+5 more"), replace(4, "mcp: none", "mcp: NONE"),
                                        replace(32, "a line 3 ", "a line X ")))]),
    ("records: xfx's tool list gains a row of tool names (the rows taken out must be the claim's)",
     "p5-transcript-records",
     [("xfx", R, "full-records", insert(3, "  \x1b[38;5;250mextra_tool,\x1b[39m", 36))]),
    ("records: fx gains a second `skills: loading` row (the rows taken out, and the rows compared, are the claim's)",
     "p5-transcript-records",
     [("fx", R, "full-records", insert(3, "  \x1b[38;5;250mskills: loading\x1b[39m", 36))]),
    ("tools: xfx's banner is gone from both screens of the one-screen record (the rows only one side covers are the claim's)",
     "p5-transcript-tools",
     [("xfx", T, snap, drop_then_pad(1, 36)) for snap in ("full-running", "full-pgup")]),
    ("question: a row is put in above xfx's banner (the same assertion, the other way)",
     "p5-transcript-question",
     [("xfx", Q, "full-120", insert(1, "stray row", 36))]),
    ("anchormid: text in the blank row above xfx's footer, on the three screens of the held page "
     "(that row is read, not dropped)", "p5-transcript-anchormid",
     [("xfx", A, snap, replace(37, "", "stray")) for snap in ("full-pgup", "full-pgup-back", "full-pgup-after")]),
]


def fresh():
    root = os.path.join(scratch, "caps")
    if os.path.exists(root):
        shutil.rmtree(root)
    for side in ("fx", "xfx"):
        shutil.copytree(os.path.join(receipt, side), os.path.join(root, side))


def checked(script, scenario):
    out = subprocess.run([sys.executable, "-I", "-B", script, repo, os.path.join(scratch, "caps"), "--only", scenario],
                         capture_output=True, text=True)
    lines = (out.stdout + out.stderr).strip().splitlines()
    fails = [l.strip() for l in lines if l.strip().startswith("FAIL ") and not l.startswith("FAIL")]
    return out.returncode, (fails[:1] or lines[-1:])


for name, scenario, changes in MUTANTS:
    fresh()
    for side, unit, snap, change in changes:
        edit(side, unit, snap, change)
    print("== mutant: %s" % name)
    code, lines = checked(old_check, scenario)
    print("   round 2 check.py: exit %d  %s" % (code, " | ".join(lines)))
    code, lines = checked(os.path.join(receipt, "check.py"), scenario)
    print("   this check.py:    exit %d  %s" % (code, " | ".join(lines)))
