"""Every launch wait, `until`, `absent` and `expect` line in a sweep's capture
logs, tallied: how long each waited, per binary.

usage: python3 -I waits.py <sweep out dir>...

Reads <out>/runs/<scenario>/<fx|xfx>/capture.log. For the launch wait (`up:`)
it gives the count, the first frames' quartiles and longest, and the dead and
failed panes; for each `until` condition and text, the count and longest wait
(and `never seen`); for `absent` and `expect`, the counts. Two lines are
singled out: the Ctrl-C exit hint's `until shown` on the line straight after
a `key C-c` (the key-to-row time `settle` allows for, capfile.py), and the
full-access warning's `until shown` as a scenario's first line after the
launch wait (the first frame's warning).
"""

import collections
import glob
import os
import re
import sys

UP = re.compile(r"^up: (a first frame|the pane is dead|the pane is gone|nothing on the screen|not waited for)(?: after (\d+\.\d\d) s)?")
UNTIL = re.compile(r"^until (shown|gone) '(.*)': (?:after (\d+\.\d\d) s, (\d+\.\d\d) s since launch(; never seen)?|still)")
ABSENT = re.compile(r"^absent '(.*)': (not on the screen|it is on the screen)")
EXPECT = re.compile(r"^expect (\S+) '(.*)': (the snap holds it|the snap does not hold it)")
HINT = "press ctrl+c again to exit"
WARNING = "Full access"
ACTION = re.compile(r"^(wait|until|absent|expect|type|paste|pastex|key|snap|snapfull|held|release|relaunch|signal|resize|title|cursor|bells|wire|sockets|home|workls|gwlog|clip|clipfile|reports|rehomefile|reseed|rmhome|models2|usage|mcphttp|clock|clipfail|clipimage) ?")


def cap_lines(scen_dir, scenario, which):
    path = os.path.join(scen_dir, scenario + ".cap")
    if not os.path.exists(path):
        path = os.path.join(scen_dir, "%s-%s.cap" % (scenario, which))
    out = []
    with open(path, encoding="utf-8") as f:
        for raw in f.read().split("\n"):
            if raw.startswith("fx: "):
                if which != "fx":
                    continue
                raw = raw[4:]
            elif raw.startswith("xfx: "):
                if which != "xfx":
                    continue
                raw = raw[5:]
            out.append(raw)
    return out


def quart(xs):
    xs = sorted(xs)
    if not xs:
        return "-"
    pick = lambda q: xs[min(len(xs) - 1, int(q * (len(xs) - 1) + 0.5))]
    return "median %.2f, p90 %.2f, max %.2f" % (pick(0.5), pick(0.9), xs[-1])


def main(outs):
    for out in outs:
        scen_dir = os.path.join(out, "scenarios-used")
        up = collections.defaultdict(list)
        upkind = collections.Counter()
        until = collections.defaultdict(list)
        never = collections.Counter()
        absent = collections.Counter()
        expect = collections.Counter()
        hint_after_key, warn_first = collections.defaultdict(list), collections.defaultdict(list)
        logs = sorted(glob.glob(os.path.join(out, "runs", "*", "*", "capture.log")))
        for log in logs:
            which = os.path.basename(os.path.dirname(log))
            scenario = os.path.basename(os.path.dirname(os.path.dirname(log)))
            with open(log, encoding="utf-8", errors="replace") as f:
                text = f.read().split("\n")
            for line in text:
                m = UP.match(line)
                if m:
                    upkind[(which, m.group(1))] += 1
                    if m.group(1) == "a first frame":
                        up[which].append(float(m.group(2)))
                m = UNTIL.match(line)
                if m and m.group(3):
                    until[(which, m.group(1), m.group(2))].append(float(m.group(3)))
                    if m.group(5):
                        never[(which, m.group(2))] += 1
                m = ABSENT.match(line)
                if m:
                    absent[(which, m.group(1), m.group(2))] += 1
                m = EXPECT.match(line)
                if m:
                    expect[(which, m.group(3))] += 1
            # The two singled-out lines, matched to the scenario's own lines.
            try:
                lines = [l for l in cap_lines(scen_dir, scenario, which) if ACTION.match(l)]
            except OSError:
                continue
            waits = [float(m.group(3)) for m in (UNTIL.match(l) for l in text) if m and m.group(3)]
            untils = [l for l in lines if l.startswith("until ")]
            if len(untils) != len(waits):
                continue
            k = 0
            for i, l in enumerate(lines):
                if not l.startswith("until "):
                    continue
                w = waits[k]
                k += 1
                if l == "until shown " + HINT and i > 0 and re.match(r"^key (?:.* )?C-c(?: |$)", lines[i - 1]):
                    hint_after_key[which].append(w)
                if l == "until shown " + WARNING and i == 0:
                    warn_first[which].append(w)
        print("## %s: %d capture logs" % (os.path.basename(out.rstrip("/")), len(logs)))
        for which in ("fx", "xfx"):
            print("%s launch wait: %s; first frames: %s" % (
                which, ", ".join("%s %d" % (k[1], n) for k, n in sorted(upkind.items()) if k[0] == which) or "no up: line",
                quart(up[which])))
        for which in ("fx", "xfx"):
            print("%s Ctrl-C hint, `until shown` straight after `key C-c`: n %d, %s" % (which, len(hint_after_key[which]), quart(hint_after_key[which])))
            print("%s full-access warning, `until shown` as the first line: n %d, %s" % (which, len(warn_first[which]), quart(warn_first[which])))
        for (which, cond, text), ws in sorted(until.items()):
            print("until %s %s %r: %d, longest %.2f s%s" % (cond, which, text, len(ws), max(ws),
                                                            "; never seen %d" % never[(which, text)] if cond == "gone" and never[(which, text)] else ""))
        for (which, text, verdict), n in sorted(absent.items()):
            print("absent %s %r: %s %d" % (which, text, verdict, n))
        for (which, verdict), n in sorted(expect.items()):
            print("expect %s: %s %d" % (which, verdict, n))


if __name__ == "__main__":
    main(sys.argv[1:])
