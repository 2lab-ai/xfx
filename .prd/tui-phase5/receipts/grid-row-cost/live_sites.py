#!/usr/bin/env python3
"""Reads a macOS `malloc_history <pid> -allBySize` report (the process run
under `MallocStackLogging=1`) and prints every live allocation site of half a
MiB or more: how many allocations, how many MiB in all, and the crate's own
frames of its stack as `file:line`, innermost first (the standard library's
and the test harness's left out). Paths are not printed: the report names the
binary's folder. usage: live_sites.py <report>"""
import re
import sys

SITE = re.compile(r"^(\d+) calls? for (\d+) bytes: (.*)")
FRAME = re.compile(r"\) (\S+)\s+(\S+\.rs:\d+)")

for line in open(sys.argv[1], encoding="utf-8", errors="replace"):
    m = SITE.match(line)
    if not m or int(m.group(2)) < 512 * 1024:
        continue
    calls, size = int(m.group(1)), int(m.group(2))
    frames = [FRAME.search(frame) for frame in m.group(3).split("|")]
    own = [f.group(2) for f in frames if f and "3xfx" in f.group(1)]
    print(f"{calls:6d} allocations {size / 1048576:6.1f} MiB  " + " < ".join(reversed(own[-8:])))
