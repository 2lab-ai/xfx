"""AT-RANK's pairs, on the committed captures: every snap of `p5-at-rank` at
40x120, fx v0.0.16 (`fx/`) against the head's xfx (`xfx/`) and the base's
(`xfx-base/`), and the five snaps of the ledger's `at-ranking` cause
(`p5-input-at`'s four, `workspace-index`'s `at-plan`), fx against the head,
cell for cell with `scripts/fxcap/cells.py` (row 1, the banner, not
compared; styles compared). A run directory on screen is `<tmp>` on every
side (at-folders round 7's `runid.py redact`). Row 1 of each side's first
capture is printed, so the builds are on record.

usage: python3 -I pairs.py   (run in the repo; writes to stdout, pairs.txt is its output)
"""

import os
import subprocess
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.realpath(__file__))
SNAPS = ["scm", "stat", "index", "file-index", "main-rs", "vercle", "abcd", "comp",
         "ab", "ta", "main-case", "empty", "road-map"]
LEDGER = [("p5-input-at", s) for s in ["at-st", "at-st-down12", "at-st-up6", "at-stat"]] + [
    ("workspace-index", "at-plan")]


def capture(side, scenario, snap):
    binary = "fx" if side == "fx" else "xfx"
    return os.path.join(HERE, side, f"{binary}-{scenario}-40x120-{snap}.ansi")


def differ(fx, xfx):
    out = subprocess.run([sys.executable, "-I", "scripts/fxcap/cells.py", fx, xfx],
                         capture_output=True, text=True)
    line = (out.stdout.strip().splitlines() or [""])[-1]
    return int(line.split()[0]) if line[:1].isdigit() else None


def say(n):
    return "identical" if n == 0 else f"{n} rows differ"


def main():
    for side in ["fx", "xfx", "xfx-base"]:
        with open(capture(side, "p5-at-rank", "scm")[:-5] + ".txt", encoding="utf-8") as f:
            print(f"# {side}/ row 1: {f.readline().strip()}")
    base_same = head_same = 0
    print(f"{'p5-at-rank':12} {'base':>14} {'head':>14}")
    for snap in SNAPS:
        fx = capture("fx", "p5-at-rank", snap)
        base, head = differ(fx, capture("xfx-base", "p5-at-rank", snap)), differ(fx, capture("xfx", "p5-at-rank", snap))
        base_same += base == 0
        head_same += head == 0
        print(f"{snap:12} {say(base):>14} {say(head):>14}")
    print(f"p5-at-rank pairs: {len(SNAPS)}; identical: base {base_same}, head {head_same}")
    ledger_same = 0
    for scenario, snap in LEDGER:
        head = differ(capture("fx", scenario, snap), capture("xfx", scenario, snap))
        ledger_same += head == 0
        print(f"{scenario} {snap:14} head {say(head)}")
    print(f"at-ranking pairs: {len(LEDGER)}; identical: head {ledger_same}")
    return 0 if head_same == len(SNAPS) and ledger_same == len(LEDGER) else 1


if __name__ == "__main__":
    sys.exit(main())
