#!/usr/bin/env python3
"""Train 12, join check 1: the catalog asks each binary made in each of the
given scenarios of a sweep, from the gateway log capture.sh published
(`gwlog`; at the head, through LOCAL-PATHS's rewrite), as SWEEP-WALLCLOCK-F's
verdicts.py writes its ask column: one character an answered ask, in order
-- 5 a 503 (down), 2 a 200 -- and `.` an ask still on its way when the log
was copied. A scenario that keeps no gateway log says so.

    python3 -I -B asks.py <sweep-out> <scenario>...
"""
import glob
import os
import re
import sys

out = sys.argv[1]
for scen in sys.argv[2:]:
    for side in ("fx", "xfx"):
        logs = glob.glob(os.path.join(out, "runs", scen, side, "*-gateway.log"))
        if not logs:
            print(f"{scen:28} {side:4} no gateway log kept")
            continue
        asked = answered = 0
        marks = []
        for line in open(logs[0], encoding="utf-8", errors="replace"):
            if re.search(r"\] GET /coding-agent/v1/models bearer=", line):
                asked += 1
            m = re.search(r"\] request GET /coding-agent/v1/models .*-> (\d{3})", line)
            if m:
                answered += 1
                marks.append({"503": "5", "200": "2"}.get(m.group(1), "?"))
        marks += ["."] * max(0, asked - answered)
        print(f"{scen:28} {side:4} {''.join(marks)}")
