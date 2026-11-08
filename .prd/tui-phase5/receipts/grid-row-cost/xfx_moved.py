#!/usr/bin/env python3
"""For every snap whose xfx screen is not the same in two sweeps (row 1, the
banner, left out, as compare.py reads them): the rows that differ, each side's
text with its escapes taken out, the row's last 60 characters -- what moved
between the base's xfx and the head's, to be read as the code's change or the
run's timing. Rows redacted as compare.py redacts them.

    python3 -I -B xfx_moved.py <base-sweep-out> <head-sweep-out>
"""
import glob
import os
import re
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "..", "scripts", "fxcap"))
import redact  # noqa: E402

REDACT = redact.Redactor({})
ESC = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)")


def screens(out):
    found = {}
    for path in glob.glob(os.path.join(out, "runs", "*", "xfx", "**", "*.ansi"), recursive=True):
        rel = os.path.relpath(path, out)
        found[rel] = open(path, encoding="utf-8", errors="replace").read().split("\n")[1:]
    return found


base, head = (screens(p) for p in sys.argv[1:3])
moved = 0
kinds = {}
for rel in sorted(set(base) & set(head)):
    if base[rel] == head[rel]:
        continue
    moved += 1
    rows = [i for i in range(max(len(base[rel]), len(head[rel])))
            if (base[rel][i:i + 1] or [""]) != (head[rel][i:i + 1] or [""])]
    print(f"{rel.replace(os.sep, '/')}: {len(rows)} rows")
    for i in rows[:3]:
        b = ESC.sub("", (base[rel][i:i + 1] or [""])[0])
        h = ESC.sub("", (head[rel][i:i + 1] or [""])[0])
        if b == h:
            print(f"    row {i + 2}: same text, other escapes")
        else:
            print(f"    row {i + 2}: base {REDACT(b)[-60:]!r}")
            print(f"    row {i + 2}: head {REDACT(h)[-60:]!r}")
print(f"# {moved} xfx screens moved")
