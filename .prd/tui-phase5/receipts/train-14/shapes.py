#!/usr/bin/env python3
"""Train 14: train 13's shapes.py, unchanged but for these words (train 14's brief has the same eight terms). Train 13 (train 12's round-2 shapes.py; the brief shape is train 13's
brief's eight terms): the three shapes pathcheck.sh and final-scan.sh read,
in one place.

  brief     the brief's pattern: a home root, the harness temp roots, the
            macOS per-user temp root, the account's name, `claude-<n>/`
  run-name  a run directory's name: `fxsw.` and six name characters, or
            `fxcap.` and six other than the masked template `XXXXXX`
  uid-tail  the harness root's uid part on its own: `claude-<digits>`, or a
            token that starts with `-<digits>/` (that root's tail, cut after
            its `claude` where a capture row wrapped it, the token's start
            a line's, a blank's, a quote's or a bracket's -- round 1's
            compare-main.txt held 14 such lines)

Each part is spelt with a bracket class, so this file matches none of them.

    ... | python3 -I -B shapes.py lines      # added lines on stdin: a count per shape
    python3 -I -B shapes.py files <path>...  # each file: its line and the shape, never the text

Exit 0 only when nothing matched.
"""
import re
import sys

SHAPES = [
    ("brief", re.compile(r"/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|zhugehyu[k]|claud[e]-[0-9]+/|-[0-9]{3}/[t][0-9]+/|fx[s][w]\.[A-Za-z0-9_]{6}")),
    ("run-name", re.compile(r"fxs[w]\.[A-Za-z0-9_]{6}|fxca[p]\.(?!XXXXXX)[A-Za-z0-9]{6}")),
    ("uid-tail", re.compile(r"claud[e]-[0-9]+|(?<![^\s'\"`(<>])-[0-9]+/")),
]
mode, paths = sys.argv[1], sys.argv[2:]
counts = {name: 0 for name, _ in SHAPES}
hits = []
if mode == "lines":
    for line in sys.stdin:
        line = line[1:] if line.startswith("+") else line  # an added line's own text
        for name, rx in SHAPES:
            counts[name] += bool(rx.search(line))
else:
    for path in paths:
        for n, line in enumerate(open(path, encoding="utf-8", errors="replace"), 1):
            for name, rx in SHAPES:
                if rx.search(line):
                    counts[name] += 1
                    hits.append(f"    {path.rsplit('/', 1)[-1]}:{n}: {name}")
print("; ".join(f"{name} {n}" for name, n in counts.items()) + (f" (over {len(paths)} files)" if mode == "files" else ""))
print("\n".join(hits)) if hits else None
sys.exit(1 if any(counts.values()) else 0)
