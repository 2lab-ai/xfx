"""p5-mcp-fail-held's timing, run by run: when the screen opened against the
failing server's tries.

usage: python3 -I held_timing.py <sweep out dir>...

Reads <out>/runs/p5-mcp-fail-held/<fx|xfx>/capture.log: the launch wait's
`up:` line (the launch to the first frame) and the `until shown` of the
server's screen (the first frame to the screen open). The tries run on the
binary's own clock, from its launch, near 7 and 15 s (a 1 s backoff that
doubles, src/mcp/runtime.rs:90), so the margins are the open against 7 s,
and the snap, `wait 1` after the open, against 15 s. A try's own length (an
`sh` that exits at once) is not counted.
"""

import os
import re
import sys

UP = re.compile(r"^up: a first frame after (\d+\.\d\d) s")
OPEN = re.compile(r"^until shown 'echo boom-stderr >&2; exit 3': after (\d+\.\d\d) s, (\d+\.\d\d) s since launch")


def main(outs):
    rows = []
    for out in outs:
        for side in ("fx", "xfx"):
            log = os.path.join(out, "runs", "p5-mcp-fail-held", side, "capture.log")
            up = opened = None
            with open(log, encoding="utf-8") as f:
                for line in f:
                    m = UP.match(line)
                    if m:
                        up = float(m.group(1))
                    m = OPEN.match(line)
                    if m:
                        opened = float(m.group(2))
            if up is None or opened is None:
                print("%s %s: no launch wait or no screen-open line" % (os.path.basename(out), side))
                continue
            at = up + opened
            rows.append((at - 7.0, 15.0 - (at + 1.0)))
            print("%s %s: launch to first frame %.2f s; first frame to screen open %.2f s; screen open %.2f s after launch: %.2f s after the try near 7 s, snap %.2f s before the try near 15 s"
                  % (os.path.basename(out), side, up, opened, at, at - 7.0, 15.0 - (at + 1.0)))
    if rows:
        print("runs %d; smallest margin after the try before %.2f s, before the try after %.2f s" % (len(rows), min(r[0] for r in rows), min(r[1] for r in rows)))


if __name__ == "__main__":
    main(sys.argv[1:])
