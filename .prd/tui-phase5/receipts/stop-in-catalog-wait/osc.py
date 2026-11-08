#!/usr/bin/env python3
"""Every OSC sequence a wire file carries, in order (the window title, OSC 2,
and the program status, OSC 7501), one per line.

    python3 -I -B osc.py <file.wire>...
"""
import re
import sys

for path in sys.argv[1:]:
    print(f"== {path.rsplit('/', 1)[-1]}")
    with open(path, "rb") as handle:
        data = handle.read()
    for match in re.finditer(rb"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)", data):
        print("  " + match.group(0)[2:].rstrip(b"\x07").rstrip(b"\x1b\\").decode("utf-8", "replace"))
