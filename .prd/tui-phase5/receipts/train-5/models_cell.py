#!/usr/bin/env python3
"""The three pairs that passed on L-U1+U2's own sweep and not on the train's,
in every sweep that took them: each pair's verdict, and for each binary
every models request the fake gateway logged (when, after the gateway
started, and what it answered) and what the snap's screen says of the
catalog. Printed through redacted.py by the caller.

    python3 -I -B models_cell.py <label>=<sweep-out>...
"""
import glob
import json
import os
import re
import sys

PAIRS = [("model-parked", "menu-loading"), ("model-reask", "menu-failed"), ("p5-input-model-fail", "fail-open")]
ASK = re.compile(r"request GET /coding-agent/v1/models at=(\d+)ms -> (\d+)")
SAYS = re.compile(r"Models \d+|Loading models…|Could not reach AI Gateway[^│\n]*")

for arg in sys.argv[1:]:
    label, out = arg.split("=", 1)
    run = json.load(open(os.path.join(out, "run.json")))
    pairs = json.load(open(os.path.join(out, "pairs.json")))
    pairs = pairs if isinstance(pairs, list) else pairs.get("pairs", pairs)
    print("## %s: xfx sha256 %s" % (label, run.get("xfx_sha256", "?")[:12]))
    for scenario, snap in PAIRS:
        verdict = [p.get("verdict") for p in pairs if p.get("scenario") == scenario and p.get("label") == snap]
        print("%s/%s: %s" % (scenario, snap, verdict[0] if verdict else "not taken"))
        for side in ("fx", "xfx"):
            logs = glob.glob(os.path.join(out, "runs", scenario, side, "*gateway.log"))
            asks = ASK.findall(open(logs[0]).read()) if logs else None
            screen = glob.glob(os.path.join(out, "runs", scenario, side, "*-%s.txt" % snap))
            said = SAYS.findall(open(screen[0]).read()) if screen else []
            print("    %-3s asks %s; the screen: %s" % (
                side, "no gateway log" if asks is None else (", ".join("%s ms %s" % a for a in asks) or "none"),
                " | ".join(said) or "-"))
