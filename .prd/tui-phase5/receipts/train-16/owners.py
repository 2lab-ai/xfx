#!/usr/bin/env python3
"""Train 16 (new): stale owner clauses, read mechanically. Every mention of
each unit this train closes -- the eleven -- in docs/parity.md, the ledger
and the tracked source (src/, tests/, scripts/) at <commit>, with the 60
characters either side, and whether `proposed`, `queued`, `owner` or `not
fixed` stand within 80 characters of it: a closed unit named as the owner of
an open difference is the clause a train must retire. Each such hit is
printed for reading; the count of hits per unit last.

    python3 -I -B owners.py <repo> <commit>
"""
import re
import subprocess
import sys

UNITS = ["PROFILE-REFUSE-AT-LAUNCH", "PROFILE-REFUSE-TYPED-KEYS", "PERMISSION-MODE-CASE", "PROFILE-JSON-LIMITS",
         "SLASH-LEADING-WS", "SLASH-ARG-SKILLS", "SLASH-MIXED-SKILLS", "TEST-ENV-ISOLATION", "LOCAL-PATHS-WRAP",
         "APPROVAL-PASTE-SPACES", "META-PREFIX-DECODE"]
NEAR = re.compile(r"proposed|queued|owner|not fixed", re.I)

repo, commit = sys.argv[1:3]
files = subprocess.run(["git", "-C", repo, "grep", "-l", "-E", "|".join(UNITS), commit, "--", "docs/parity.md",
                        "scripts/fxcap/parity-ledger.toml", "src", "tests", "scripts"],
                       capture_output=True, text=True).stdout.split()
files = [f.split(":", 1)[1] for f in files]
counts = {u: [0, 0] for u in UNITS}
print(f"# owners.py at {commit[:8]}: {len(files)} files name a unit of the eleven")
for f in files:
    text = subprocess.run(["git", "-C", repo, "show", f"{commit}:{f}"], capture_output=True, text=True).stdout
    for n, line in enumerate(text.split("\n"), 1):
        for u in UNITS:
            for m in re.finditer(re.escape(u) + r"(?![A-Z-])", line):
                counts[u][0] += 1
                near = line[max(0, m.start() - 80):m.end() + 80]
                if NEAR.search(near):
                    counts[u][1] += 1
                    print(f"{f}:{n} {u}: ...{line[max(0, m.start() - 60):m.end() + 60]}...")
print("# per unit: mentions, of them with an owner word within 80 characters")
for u, (a, b) in counts.items():
    print(f"{u}: {a}, {b}")
