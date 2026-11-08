#!/usr/bin/env python3
"""The three `modelsdown` scenarios run by run: each side's catalog asks (ms
after the fake gateway started, and the answer) beside each pair's verdict --
whether the pair passes follows which side of the outage's end each binary's
reads landed on. usage: window.py <label>=<sweep-dir>..."""
import json, os, re, sys
SC = {"model-reask": 6000, "model-column-idle": 9000, "model-column-reask": 9000}
for arg in sys.argv[1:]:
    label, d = arg.split("=", 1)
    pairs = {(x["scenario"], x["label"]): x["verdict"] for x in json.load(open(f"{d}/pairs.json"))}
    for sc, down in SC.items():
        if not any(k[0] == sc for k in pairs):
            continue
        sides = []
        for side in ("fx", "xfx"):
            log = os.path.join(d, "runs", sc, side, f"{side}-{sc}-40x120-gateway.log")
            asks = re.findall(r"request GET /coding-agent/v1/models at=(\d+)ms -> (\d+)", open(log).read()) if os.path.exists(log) else []
            sides.append(f"{side} " + (", ".join(f"{a}:{c}" for a, c in asks) or "-"))
        verdicts = ", ".join(f"{lab} {'pass' if v in ('identical', 'normalized-identical') else 'FAIL'}" for (s, lab), v in sorted(pairs.items()) if s == sc and lab != "start")
        print(f"{label:<14} {sc:<19} down<{down}ms | {sides[0]:<28} | {sides[1]:<28} | {verdicts}")
