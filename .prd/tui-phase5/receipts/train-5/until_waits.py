#!/usr/bin/env python3
"""Every `until` line capture.sh logged in the given sweeps, for the swept
scenarios that have one: how long each waited after its line, and the time
since launch. Printed through redacted.py by the caller.

    python3 -I -B until_waits.py <label>=<sweep-out>...
"""
import glob
import os
import sys

for arg in sys.argv[1:]:
    label, out = arg.split("=", 1)
    print("## %s" % label)
    for log in sorted(glob.glob(os.path.join(out, "runs", "*", "*", "capture.log"))):
        side = os.path.basename(os.path.dirname(log))
        scenario = os.path.basename(os.path.dirname(os.path.dirname(log)))
        for line in open(log, errors="replace"):
            if line.startswith("until "):
                print("%s\t%s\t%s\t%s" % (label, scenario, side, line.rstrip("\n")))
