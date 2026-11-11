"""AT-RANK's pairs again, at AT-RANK-PERF's build: every snap of `p5-at-rank`
at 40x120 and the five snaps of the ledger's old `at-ranking` cause
(`p5-input-at`'s four, `workspace-index`'s `at-plan`), fx v0.0.16 as AT-RANK
captured it (`../at-rank/fx/`, the same pin and binary) against this
unit's xfx (`xfx/`), cell for cell with `scripts/fxcap/cells.py` (row 1, the
banner, not compared; styles compared). A run directory on screen is
`<tmp>` on every side (at-folders round 7's `runid.py redact`). Row 1 of
each side's first capture is printed, so the builds are on record.
(AT-RANK's `../at-rank/pairs.py`, with this unit's xfx side and no base.)

usage: python3 -I pairs.py   (run in the repo; writes to stdout, pairs.txt is its output)
"""

import os
import subprocess
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.realpath(__file__))
FX = os.path.join(HERE, "..", "at-rank", "fx")
XFX = os.path.join(HERE, "xfx")
SNAPS = ["scm", "stat", "index", "file-index", "main-rs", "vercle", "abcd", "comp",
         "ab", "ta", "main-case", "empty", "road-map"]
LEDGER = [("p5-input-at", s) for s in ["at-st", "at-st-down12", "at-st-up6", "at-stat"]] + [
    ("workspace-index", "at-plan")]


def capture(side, scenario, snap):
    binary = "fx" if side == FX else "xfx"
    return os.path.join(side, f"{binary}-{scenario}-40x120-{snap}.ansi")


def differ(fx, xfx):
    out = subprocess.run([sys.executable, "-I", "scripts/fxcap/cells.py", fx, xfx],
                         capture_output=True, text=True)
    line = (out.stdout.strip().splitlines() or [""])[-1]
    return int(line.split()[0]) if line[:1].isdigit() else None


def say(n):
    return "identical" if n == 0 else f"{n} rows differ"


def main():
    for name, side in [("../at-rank/fx", FX), ("xfx", XFX)]:
        with open(capture(side, "p5-at-rank", "scm")[:-5] + ".txt", encoding="utf-8") as f:
            print(f"# {name}/ row 1: {f.readline().strip()}")
    same = 0
    for snap in SNAPS:
        n = differ(capture(FX, "p5-at-rank", snap), capture(XFX, "p5-at-rank", snap))
        same += n == 0
        print(f"p5-at-rank {snap:16} {say(n)}")
    print(f"p5-at-rank pairs: {len(SNAPS)}; identical: {same}")
    ledger_same = 0
    for scenario, snap in LEDGER:
        n = differ(capture(FX, scenario, snap), capture(XFX, scenario, snap))
        ledger_same += n == 0
        print(f"{scenario} {snap:14} {say(n)}")
    print(f"at-ranking pairs: {len(LEDGER)}; identical: {ledger_same}")
    return 0 if same == len(SNAPS) and ledger_same == len(LEDGER) else 1


if __name__ == "__main__":
    sys.exit(main())
