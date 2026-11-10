"""CTRLO-AT-KEYS' pairs: every snap of `p5-ctrlo-at-keys` at 40x120, fx
v0.0.16 (`fx/`) against the head's xfx (`xfx/`) and the base's
(`xfx-base/`), cell for cell with `scripts/fxcap/cells.py` (row 1, the
banner, not compared). Row 1 of each side's first capture is printed, so
the builds the captures came from are on record.

usage: python3 -I pairs.py   (run in the repo; writes to stdout, pairs.txt is its output)
"""

import os
import subprocess
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.realpath(__file__))
SNAPS = ["open", "tab", "down", "esc", "letter", "tail-open", "ck", "cj"]
NAME = "p5-ctrlo-at-keys-40x120-{snap}.{ext}"


def capture(side, binary, snap, ext):
    return os.path.join(HERE, side, f"{binary}-" + NAME.format(snap=snap, ext=ext))


def differ(fx, xfx):
    out = subprocess.run(
        [sys.executable, "-I", "scripts/fxcap/cells.py", fx, xfx],
        capture_output=True, text=True,
    )
    line = (out.stdout.strip().splitlines() or [""])[-1]
    return int(line.split()[0]) if line[:1].isdigit() else None, line


def main():
    for side, binary in [("fx", "fx"), ("xfx", "xfx"), ("xfx-base", "xfx")]:
        with open(capture(side, binary, "open", "txt"), encoding="utf-8") as f:
            print(f"# {side}/ row 1: {f.readline().strip()}")
    head_same = base_same = 0
    print(f"{'snap':10} {'base':>14} {'head':>14}")
    for snap in SNAPS:
        fx = capture("fx", "fx", snap, "ansi")
        base, _ = differ(fx, capture("xfx-base", "xfx", snap, "ansi"))
        head, _ = differ(fx, capture("xfx", "xfx", snap, "ansi"))
        base_same += base == 0
        head_same += head == 0
        say = lambda n: "identical" if n == 0 else f"{n} rows differ"
        print(f"{snap:10} {say(base):>14} {say(head):>14}")
    print(f"pairs: {len(SNAPS)}; identical: base {base_same}, head {head_same}")
    return 0 if head_same == len(SNAPS) else 1


if __name__ == "__main__":
    sys.exit(main())
