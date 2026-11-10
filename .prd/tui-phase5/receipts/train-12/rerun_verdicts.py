#!/usr/bin/env python3
"""Train 12: each rerun's verdicts (sweep-rerun.sh's `<prefix>-<i>`
directories, one scenario each), pair by pair: the verdict in every rerun,
how many passed, and -- for model-column-idle -- whether each binary's snap
shows the catalog's failure sentence or its rows (`unable to load models`,
`gpt-6-astra`), the states SWEEP-WALLCLOCK-F's scenario is built to show;
for p5-mcp-menu-new, the state its `list-remote` snap shows for the server
nothing listens for (`connecting` or `retrying`).

    python3 -I -B rerun_verdicts.py <rerun-dir> <name>-<scenario>...

A `sweep-<name>-<scenario>=<sweep-out>` argument reads one scenario of a
whole sweep as a single run (its pairs and states in the head's and main's
sweeps).
"""
import glob
import json
import os
import sys

PASS = ("identical", "normalized-identical")
SHORT = {"identical": "ID", "normalized-identical": "NI", "unresolved-defect": "UD",
         "approved-divergence": "AD", "approved-divergence-cascade": "ADC", "failed": "F", "timeout": "T"}
root = sys.argv[1]
for prefix in sys.argv[2:]:
    only = None
    if "=" in prefix:
        prefix, sweep = prefix.split("=", 1)
        runs = [sweep]
        only = prefix.split("-", 2)[2]  # sweep-<name>-<scenario>
    else:
        runs = sorted(glob.glob(os.path.join(root, prefix + "-[0-9]")), key=lambda p: int(p.rsplit("-", 1)[1]))
    table = {}
    for i, run in enumerate(runs, 1):
        for p in json.load(open(os.path.join(run, "pairs.json"), encoding="utf-8")):
            if only is not None and p["scenario"] != only:
                continue
            table.setdefault(p["label"], {})[i] = p["verdict"]
    print(f"## {prefix}: {len(runs)} reruns")
    for label, by in table.items():
        cells = [SHORT.get(by.get(i, "-"), by.get(i, "-")) for i in range(1, len(runs) + 1)]
        print(f"    {label:16} {' '.join(cells)}  pass {sum(by.get(i) in PASS for i in range(1, len(runs) + 1))}/{len(runs)}")
    if prefix.endswith("p5-mcp-menu-new"):
        for i, run in enumerate(runs, 1):
            states = []
            for side in ("fx", "xfx"):
                txt = glob.glob(os.path.join(run, "runs", "p5-mcp-menu-new", side, f"{side}-p5-mcp-menu-new-*-list-remote.txt"))
                line = next((l for l in open(txt[0], encoding="utf-8") if "remote  " in l), "") if txt else ""
                states.append(f"{side} {line.split()[2] if len(line.split()) > 2 else '?'}")
            print(f"    rerun {i}: list-remote's `remote` row: " + "; ".join(states))
    if prefix.endswith("model-column-idle"):
        for i, run in enumerate(runs, 1):
            states = []
            for label in ("col-down", "col-idle", "col-typed"):
                for side in ("fx", "xfx"):
                    txt = glob.glob(os.path.join(run, "runs", "model-column-idle", side, f"{side}-model-column-idle-*-{label}.txt"))
                    text = open(txt[0], encoding="utf-8").read() if txt else ""
                    state = "failure" if "unable to load models" in text else "rows" if "gpt-6-astra" in text else "?"
                    states.append(f"{label} {side} {state}")
            print(f"    rerun {i}: " + "; ".join(states))
