"""Rounds 3 and 4: one mutant per assertion `check.py` gained, run through the
`check.py` of the round before it and through this one -- and **asserted**:
the run fails unless every checker passes the unchanged captures, the round
before passes each mutant (so the assertion is new), and this `check.py`
fails each mutant with the FAIL line it names (so it is that assertion that
catches it, not a crash).

usage: python3 -I -B mutants-c.py <repo> <receipt-dir> <scratch-dir> \
           <round-2-check.py> <round-3-check.py>

Round 2's `check.py` is `f103c2a9`'s, round 3's `6eee9fb9`'s (`git show
<rev>:.prd/tui-phase5/receipts/transcript-wrap/check.py`). Round 3
(T-U1U2c) added the first six assertions; round 4 (T-U6) pinned a record's
height and the row and height of an inserted one.

Each mutant copies the receipt's captures into <scratch-dir>, changes rows of
one or two captures (keeping each screen's 40 rows), and runs the checkers on
the scenario. Nothing in the receipt or the repository is written. Exit 0
only when every assertion above holds.
"""

import os
import shutil
import subprocess
import sys

repo, receipt, scratch, round2, round3 = (os.path.realpath(p) for p in sys.argv[1:6])
EARLIER = {3: ("round 2", round2), 4: ("round 3", round3)}


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
    the removal)."""
    def change(lines):
        del lines[row - 1]
        lines.insert(pad_at - 1, "")
        return lines
    return change


def move(row, to):
    """Row `row` taken out and put in again as row `to` (counted after the
    removal): the rows between move up one."""
    def change(lines):
        moved = lines.pop(row - 1)
        lines.insert(to - 1, moved)
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
N = "p5-transcript-anchor-40x120"
# (round, name, scenario, changes, the FAIL line this check.py must print)
MUTANTS = [
    (3, "records: xfx's tool list loses its closing `)`, and a row after it and one far below are changed "
     "(the tool list must close; gpt6-b's MUST)", "p5-transcript-records",
     [("xfx", R, "full-records", chain(replace(3, "+5 more)", "+5 more"), replace(4, "mcp: none", "mcp: NONE"),
                                        replace(32, "a line 3 ", "a line X ")))],
     "FAIL malformed capture: xfx: the tool list from row 2 has no closing row: '  +5 more'"),
    (3, "records: xfx's tool list gains a row of tool names (the rows taken out must be the claim's)",
     "p5-transcript-records",
     [("xfx", R, "full-records", insert(3, "  \x1b[38;5;250mextra_tool,\x1b[39m", 36))],
     "FAIL full-records: xfx: 3 tools-list rows taken out, the claim is 2"),
    (3, "records: fx gains a second `skills: loading` row (the rows taken out, and the rows compared, are the claim's)",
     "p5-transcript-records",
     [("fx", R, "full-records", insert(3, "  \x1b[38;5;250mskills: loading\x1b[39m", 36))],
     "FAIL full-records: fx: 3 session-record rows taken out, the claim is 2"),
    (3, "tools: xfx's banner is gone from both screens of the one-screen record (the rows only one side covers are the claim's)",
     "p5-transcript-tools",
     [("xfx", T, snap, drop_then_pad(1, 36)) for snap in ("full-running", "full-pgup")],
     "FAIL only fx covers 1 and only xfx 0, the claim is 0 and 0"),
    (3, "question: a row is put in above xfx's banner (the same assertion, the other way)",
     "p5-transcript-question",
     [("xfx", Q, "full-120", insert(1, "stray row", 36))],
     "FAIL only fx covers 0 and only xfx 1, the claim is 0 and 0"),
    (3, "anchormid: text in the blank row above xfx's footer, on the three screens of the held page "
     "(that row is read, not dropped)", "p5-transcript-anchormid",
     [("xfx", A, snap, replace(37, "", "stray")) for snap in ("full-pgup", "full-pgup-back", "full-pgup-after")],
     "FAIL malformed capture: the row above the footer is 'stray', not a blank one"),
    (4, "tools: xfx's footer one row up on both screens of the one-screen record, a blank record row "
     "taken out and one put in under the hint (a record is 36 rows)", "p5-transcript-tools",
     [("xfx", T, snap, drop_then_pad(36, 40)) for snap in ("full-running", "full-pgup")],
     "FAIL malformed capture: the footer is on row 37, not row 38"),
    (4, "anchor: the network record lands a row higher on xfx, the blank row above it moved under it "
     "(the row it is put in at is the claim's)", "p5-transcript-anchor",
     [("xfx", N, "full-pgup2-after", move(18, 23))],
     "FAIL xfx: a 5-row record at row 18, the claim is a 5-row record at row 19"),
    (4, "anchor: xfx's network record gains a row, the last record row taken out (its height is the claim's)",
     "p5-transcript-anchor",
     [("xfx", N, "full-pgup2-after", insert(21, "  \x1b[38;5;250mextra: row\x1b[39m", 36))],
     "FAIL xfx: a 6-row record at row 19, the claim is a 5-row record at row 19"),
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
    return out.returncode, fails, (fails[:1] or lines[-1:])


this = os.path.join(receipt, "check.py")
broken = []
scenarios = sorted({m[2] for m in MUTANTS})
print("== unchanged captures")
fresh()
for scenario in scenarios:
    for label, script in (("round 2", round2), ("round 3", round3), ("this", this)):
        code, _, shown = checked(script, scenario)
        print("   %-26s %-8s exit %d  %s" % (scenario, label, code, " | ".join(shown)))
        if code != 0:
            broken.append("%s fails the unchanged %s" % (label, scenario))

for new_in, name, scenario, changes, expected in MUTANTS:
    fresh()
    for side, unit, snap, change in changes:
        edit(side, unit, snap, change)
    print("== round %d mutant: %s" % (new_in, name))
    label, script = EARLIER[new_in]
    code, _, shown = checked(script, scenario)
    print("   %s check.py: exit %d  %s" % (label, code, " | ".join(shown)))
    if code != 0:
        broken.append("%s already fails %r: the assertion is not new" % (label, name))
    code, fails, shown = checked(this, scenario)
    print("   this check.py:    exit %d  %s" % (code, " | ".join(shown)))
    if code != 1 or expected not in fails:
        broken.append("this check.py does not fail %r with %r (exit %d, %s)" % (name, expected, code, fails))

print("%d mutants; %s" % (len(MUTANTS), "every assertion held" if not broken else "%d assertions broke:" % len(broken)))
for line in broken:
    print("   BROKEN " + line)
sys.exit(1 if broken else 0)
