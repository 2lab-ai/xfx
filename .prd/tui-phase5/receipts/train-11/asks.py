#!/usr/bin/env python3
"""Train 11 (train 10's asks.py, unchanged but for this line): when each binary asked the fake Gateway's catalog in a scenario's run, and
when it answered (the run's gateway logs; ms after the fake Gateway started),
with the pair's verdict: for model-parked, whose catalog is down for its first
6000 ms (`modelsdown 6000`, wall-clock -- the ledger's `catalog-window`
cause, FLAKE-MODELSDOWN), so a binary whose first ask falls past the outage
loads at once and one inside it waits for its retry.

    python3 -I -B asks.py <scenario> <label> <name>=<sweep-out>...
"""
import glob
import json
import re
import sys

scenario, label = sys.argv[1:3]
for arg in sys.argv[3:]:
    name, out = arg.split("=", 1)
    pairs = {(p["scenario"], p["label"]): p for p in json.load(open(f"{out}/pairs.json"))}
    verdict = pairs[(scenario, label)]["verdict"]
    cells = []
    for side in ("fx", "xfx"):
        logs = glob.glob(f"{out}/runs/{scenario}/{side}/*gateway.log")
        text = open(logs[0]).read() if logs else ""
        asked = re.findall(r"GET /coding-agent/v1/models bearer=\S+ at=(\d+)ms", text)
        answered = re.findall(r"GET /coding-agent/v1/models -> (\d+) at=(\d+)ms", text)
        cells.append(f"{side} asked at {', '.join(a + ' ms' for a in asked)}; answered "
                     + ", ".join(f"{s} at {t} ms" for s, t in answered))
    print(f"{name}: {scenario} {label} {verdict}; " + "; ".join(cells))
