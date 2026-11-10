#!/usr/bin/env python3
"""Train 12, join checks 1 and 4: every file a sweep's captures left (its
`runs/` tree: snaps, wires, gateway logs, request bodies, reports), read for
a path of the machine the sweep ran on -- the brief's pattern (each part
spelt with a bracket class so this file does not match itself), the sweep's
own TMPDIR and its resolved form (given), a run directory's raw name
(`fxcap.` and six name characters, the masked `XXXXXX` apart; `fxsw.`), and
the account's name (read at run time, never written here). Nothing found is
printed but its kind and file; the scenarios named first are listed one by
one, every other scenario summed. Two files in each side's directory are
not capture.sh's: `capture.log`, its console (the out paths it wrote, the
fx guard's line naming the oracle's path), and `status.json`, sweep.py's;
a sweep commits neither but through redact.py. They are counted apart.

    python3 -I -B capture_scan.py <sweep-out> <tmpdir> [<scenario>...]
"""
import os
import pwd
import re
import sys

out, tmp = sys.argv[1], sys.argv[2]
named = sys.argv[3:]
ME = pwd.getpwuid(os.getuid()).pw_name
SHAPES = [
    ("the brief's pattern", re.compile(rb"/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|claud[e]-[0-9]+/")),
    ("a run directory's name", re.compile(rb"fxca[p]\.(?!XXXXXX)[A-Za-z0-9]{6}|fxs[w]\.[A-Za-z0-9_]{6}")),
    ("the account's name", re.compile(re.escape(ME.encode()))),
]
roots = {tmp.rstrip("/").encode(), os.path.realpath(tmp).encode()}
SHAPES.append(("the sweep's TMPDIR", re.compile(b"|".join(re.escape(r) for r in sorted(roots, key=len, reverse=True)))))
runs = os.path.join(out, "runs")
DRIVER = ("capture.log", "status.json")
per = {}
driver = [0, 0]
for scenario in sorted(os.listdir(runs)):
    files = hits = 0
    found = []
    for base, _, names in os.walk(os.path.join(runs, scenario)):
        for name in names:
            path = os.path.join(base, name)
            data = open(path, "rb").read()
            kinds = [k for k, rx in SHAPES if rx.search(data)]
            if name in DRIVER:
                driver[0] += 1
                driver[1] += bool(kinds)
                continue
            files += 1
            if kinds:
                hits += 1
                found.append((os.path.relpath(path, runs), kinds))
    per[scenario] = (files, hits, found)
total_files = sum(v[0] for v in per.values())
total_hits = sum(v[1] for v in per.values())
for scenario in named:
    files, hits, found = per.get(scenario, (0, 0, []))
    print(f"{scenario}: {files} files, {hits} holding a path of this machine")
    for rel, kinds in found:
        print(f"    {rel}: {', '.join(kinds)}")
rest = [s for s in per if s not in named]
print(f"# every other scenario ({len(rest)}): {sum(per[s][0] for s in rest)} files, {sum(per[s][1] for s in rest)} holding one")
for s in rest:
    for rel, kinds in per[s][2]:
        print(f"    {rel}: {', '.join(kinds)}")
print(f"# all {len(per)} scenarios: {total_files} files capture.sh wrote, {total_hits} holding a path of this machine")
print(f"# the driver's own files (capture.log, status.json), apart: {driver[0]}, {driver[1]} holding one")
sys.exit(1 if total_hits else 0)
