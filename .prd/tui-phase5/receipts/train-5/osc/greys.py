#!/usr/bin/env python3
"""TC-NOTATION's greys against the frames L-U1+U2's checker reads: for each
binary and scenario, under the capture's own environment and under
`COLORTERM=truecolor` (the `-tc` twins), the frames on the scenario's last
wire (each wire is every byte so far, so the last holds the rest), and in
them:

- how many SGR sequences, and how many with a direct colour (`38;2`/`48;2`);
- each word osc.py's `order` claim reads (`Thinking`, `Compaction failed`):
  in how many frames osc.py finds it (escape sequences set aside, as it does)
  and in how many it is in the bytes whole, with no escape sequence inside
  it -- so a word painted a glyph at a time, which a truecolor grey could
  split, would show as found by osc.py and not whole.

And for xfx and for fx, whether the SGR sequences the frames use are the same
set under the twin as under the capture's own environment (told, not counted:
two runs paint at their own moments). A problem is a word osc.py finds that
is not whole in the bytes, on either binary, or a direct colour in an xfx
frame.

    python3 -I -B greys.py <osc-dir>

reads <osc-dir>/default/<fx|xfx>/<scenario>-40x120/ and
<osc-dir>/tc/<fx|xfx>/<scenario>-tc-40x120/.
"""
import glob
import os
import re
import sys

FRAME = re.compile(rb"\x1b\[\?2026h(.*?)\x1b\[\?2026l", re.S)
SEQUENCE = re.compile(rb"\x1b\[[0-9;?<>=]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")
SGR = re.compile(rb"\x1b\[([0-9;:]*)m")
WORDS = (b"Thinking", b"Compaction failed")


def direct(params):
    p = params.replace(b":", b";").split(b";")
    return any(p[i] in (b"38", b"48") and i + 1 < len(p) and p[i + 1] == b"2" for i in range(len(p)))


def last_wire(d):
    wires = sorted(glob.glob(os.path.join(d, "*.wire")), key=os.path.getsize)
    return open(wires[-1], "rb").read() if wires else None


def read(data):
    frames = [m.group(1) for m in FRAME.finditer(data)]
    sgrs = [m.group(1) for f in frames for m in SGR.finditer(f)]
    out = {"frames": len(frames), "sgr": len(sgrs), "direct": sum(direct(s) for s in sgrs), "set": set(sgrs)}
    for w in WORDS:
        out[w] = (sum(bool(re.search(rb"\b" + w + rb"\b", SEQUENCE.sub(b" ", f))) for f in frames),
                  sum(w in f for f in frames))
    return out


def main():
    root = sys.argv[1]
    bad = 0
    units = sorted(os.path.basename(d)[: -len("-40x120")] for d in glob.glob(os.path.join(root, "default", "xfx", "*-40x120")))
    print("%-34s %-4s %-7s %7s %6s %6s  %-22s %-22s" % ("scenario", "bin", "env", "frames", "SGR", "38;2",
                                                       "Thinking osc.py/whole", "Compaction failed"))
    for u in units:
        for side in ("fx", "xfx"):
            seen = {}
            for env, d in (("default", os.path.join(root, "default", side, u + "-40x120")),
                           ("tc", os.path.join(root, "tc", side, u + "-tc-40x120"))):
                data = last_wire(d)
                if data is None:
                    print("%-34s %-4s %-7s no wire" % (u, side, env))
                    bad += 1
                    continue
                r = read(data)
                seen[env] = r
                print("%-34s %-4s %-7s %7d %6d %6d  %-22s %-22s" % (
                    u, side, env, r["frames"], r["sgr"], r["direct"],
                    "%d/%d" % r[b"Thinking"], "%d/%d" % r[b"Compaction failed"]))
                for w in WORDS:
                    if r[w][0] != r[w][1]:
                        print("    SPLIT: %s found by osc.py in %d frames, whole in %d" % (w.decode(), *r[w]))
                        bad += 1
                if side == "xfx" and r["direct"]:
                    print("    DIRECT: xfx frames carry %d direct colours" % r["direct"])
                    bad += 1
            if len(seen) == 2:
                extra = seen["tc"]["set"] - seen["default"]["set"]
                gone = seen["default"]["set"] - seen["tc"]["set"]
                if extra or gone:
                    # Told, not counted: two runs paint at their own moments
                    # (a blink phase, a clock), so the sets can differ by the
                    # run; a direct colour is counted above.
                    print("    SGR set differs under truecolor: +%s -%s" % (sorted(extra)[:6], sorted(gone)[:6]))
    print("OK" if not bad else "%d problems" % bad)
    return 1 if bad else 0


sys.exit(main())
