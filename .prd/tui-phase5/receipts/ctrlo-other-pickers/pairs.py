"""CTRLO-OTHER-PICKERS' pairs (round 2): every snap of the unit's ten
scenarios at 40x120, fx v0.0.16 (`fx/`) against the head's xfx (`xfx/`),
round 1's (`xfx-r1/`) and the base's (`xfx-base/`, `-` for a snap round 2
added), cell for cell with `scripts/fxcap/cells.py` (row 1, the
banner, not compared). Row 1 of each side's first capture is printed, so
the builds the captures came from are on record. A pair is `same-as-open`
when the rows that differ are, row for row, the rows the scenario's `open`
snap differs in -- taken before any Ctrl-O, so a difference outside this
unit (the help catalog's command count, the skills tabs, the provider
rows); any other head pair that differs has its rows printed under it. The
sweep's ledger classifies them all (`sweep-compare.txt`).

usage: python3 -I pairs.py   (run in the repo; writes to stdout, pairs.txt is its output)
A report, not a gate: it exits 0; the verdicts are the sweep's.
"""

import os
import subprocess
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.realpath(__file__))
SCENARIOS = {
    "p5-ctrlo-help": ["open", "held", "down", "ck", "cj", "tab", "esc", "burst-ck", "burst-cj"],
    "p5-ctrlo-model": ["open", "held", "down", "ck", "cj", "tab", "esc"],
    "p5-ctrlo-sessions": ["open", "held", "down", "ck", "cj", "tab", "esc"],
    "p5-ctrlo-fork": ["open", "held", "up", "ck", "cj", "tab", "esc", "burst-ck", "burst-cj"],
    "p5-ctrlo-effort": ["open", "held", "down", "ck", "cj", "tab", "esc"],
    "skills-ctrlo": ["open", "held", "down", "ck", "cj", "tab", "esc"],
    "skills-ctrlo-dollar": ["open", "held", "down", "ck", "cj", "esc"],
    "p5-ctrlo-slash": ["open", "down", "esc", "ck", "cj"],
    "provider-ctrlo": ["open", "down", "esc", "ck", "cj", "bspace", "alt-enter"],
    "p5-ctrlo-mcp-args": ["open", "down", "esc", "ck", "cj"],
}


def capture(side, binary, scenario, snap, ext):
    return os.path.join(HERE, side, f"{binary}-{scenario}-40x120-{snap}.{ext}")


def rows(path):
    with open(path, encoding="utf-8") as f:
        return f.read().split("\n")


def differ(fx, xfx):
    out = subprocess.run(
        [sys.executable, "-I", "scripts/fxcap/cells.py", fx, xfx],
        capture_output=True, text=True,
    )
    line = (out.stdout.strip().splitlines() or [""])[-1]
    return int(line.split()[0]) if line[:1].isdigit() else None


def text_rows(fx, xfx):
    """The rows (1-based, row 1 skipped) whose text differs."""
    a, b = rows(fx), rows(xfx)
    return {i + 1 for i in range(1, max(len(a), len(b))) if (a[i] if i < len(a) else None) != (b[i] if i < len(b) else None)}


def main():
    for side, binary in [("fx", "fx"), ("xfx", "xfx"), ("xfx-r1", "xfx"), ("xfx-base", "xfx")]:
        with open(capture(side, binary, "p5-ctrlo-help", "open", "txt"), encoding="utf-8") as f:
            print(f"# {side}/ row 1: {f.readline().strip()}")
    total = base_same = r1_same = head_same = head_open = 0
    print(f"{'scenario':22} {'snap':9} {'base':>14} {'round 1':>14} {'head':>14}")
    for scenario, snaps in SCENARIOS.items():
        open_rows = text_rows(capture("fx", "fx", scenario, "open", "txt"), capture("xfx", "xfx", scenario, "open", "txt"))
        for snap in snaps:
            fx = capture("fx", "fx", scenario, snap, "ansi")
            base_path = capture("xfx-base", "xfx", scenario, snap, "ansi")
            base = differ(fx, base_path) if os.path.exists(base_path) else None
            r1 = differ(fx, capture("xfx-r1", "xfx", scenario, snap, "ansi"))
            head = differ(fx, capture("xfx", "xfx", scenario, snap, "ansi"))
            head_rows = text_rows(capture("fx", "fx", scenario, snap, "txt"), capture("xfx", "xfx", scenario, snap, "txt"))
            total += 1
            base_same += base == 0
            r1_same += r1 == 0
            head_same += head == 0
            as_open = head != 0 and head == len(open_rows) and head_rows == open_rows
            head_open += as_open
            say = lambda n: "-" if n is None else ("identical" if n == 0 else f"{n} rows differ")
            print(f"{scenario:22} {snap:9} {say(base):>14} {say(r1):>14} {say(head):>14}{'  same-as-open' if as_open else ''}")
            if head != 0 and not as_open:
                fx_rows = rows(capture("fx", "fx", scenario, snap, "txt"))
                xfx_rows = rows(capture("xfx", "xfx", scenario, snap, "txt"))
                for row in sorted(head_rows):
                    print(f"    row {row}: fx {fx_rows[row - 1].rstrip()!r}")
                    print(f"    row {row}: xfx {xfx_rows[row - 1].rstrip()!r}")
    print(f"pairs: {total}; identical: base {base_same}, round 1 {r1_same}, head {head_same}; head same-as-open: {head_open}; other: {total - head_same - head_open}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
