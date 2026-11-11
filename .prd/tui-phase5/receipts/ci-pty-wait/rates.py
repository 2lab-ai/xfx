#!/usr/bin/env python3
"""Rates per build and test from a starve.sh run, and the study build's paste
pauses beside each hostile-paste run's outcome.

    python3 -I rates.py <starve-out-dir> <rates.md> <study.txt>

The study build is the head's harness without the retry: it prints
`STUDY-GAP <microseconds>` (the longest pause inside its paste) and then makes
the assertions whatever the pause was. Prints no path.
"""
import collections
import os
import re
import sys

out_dir, rates_md, study_txt = sys.argv[1:4]
runs = collections.defaultdict(lambda: [0, 0, []])
for line in open(os.path.join(out_dir, "results.txt")):
    round_, label, name, code, secs = line.split()
    entry = runs[(label, name)]
    entry[0] += 1
    entry[1] += code != "0"
    entry[2].append(int(secs))

labels = sorted({label for label, _ in runs}, key=lambda l: ["base", "study", "head"].index(l) if l in ("base", "study", "head") else 9)
names = sorted({name for _, name in runs})
with open(rates_md, "w") as md:
    md.write("| test | " + " | ".join(f"{l}: failed / runs (max s)" for l in labels) + " |\n")
    md.write("|---|" + "---|" * len(labels) + "\n")
    for name in names:
        cells = []
        for label in labels:
            n, bad, secs = runs.get((label, name), [0, 0, [0]])
            cells.append(f"{bad} / {n} ({max(secs)})")
        md.write(f"| `{name}` | " + " | ".join(cells) + " |\n")

with open(study_txt, "w") as st:
    st.write("study build (a diagnostic: the working tree between f79f5e5b and f311ad79 -- the product fix in, the harness measuring the paste's longest pause and asserting whatever it was, no retry): one line per hostile-paste run --\n")
    st.write("outcome, longest pause inside the paste (ms), and for a failure what failed\n")
    rows = []
    for log in sorted(os.listdir(out_dir)):
        if not (log.startswith("study.a_mebibyte") and log.endswith(".log")):
            continue
        text = open(os.path.join(out_dir, log), errors="replace").read()
        gap = re.search(r"STUDY-GAP (\d+)", text)
        ok = "test result: ok." in text
        what = ""
        if not ok:
            m = re.search(r"panicked at ([^\n]+)\n([^\n]{0,160})", text)
            what = (m.group(1) + " " + m.group(2)) if m else "no panic line"
            keys = len(set(re.findall(r"K\d{7}1", text)))
            what += f" | body tokens on the screen: {keys}"
        rows.append((ok, int(gap.group(1)) / 1000 if gap else None, log.split(".")[1], what))
    for ok, gap, name, what in sorted(rows, key=lambda r: (r[0], -(r[1] or 0))):
        gap_s = f"{gap:.1f}" if gap is not None else "none"
        st.write(f"{'pass' if ok else 'FAIL'} gap_ms={gap_s} {name} {what}\n")
    fails = [r for r in rows if not r[0]]
    paused = [r for r in rows if r[1] is not None and r[1] >= 50]
    st.write(f"\nruns {len(rows)}; failed {len(fails)}; paused >= 50 ms {len(paused)}; "
             f"failed with a pause >= 50 ms {sum(1 for r in fails if r[1] is not None and r[1] >= 50)}; "
             f"failed with every pause < 50 ms {sum(1 for r in fails if r[1] is not None and r[1] < 50)}; "
             f"passed with a pause >= 50 ms {sum(1 for r in paused if r[0])}\n")
