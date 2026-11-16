#!/usr/bin/env python3
"""Every fxcap scenario that seeds a profile before launch, and whether fx
v0.0.16 refuses that profile at launch (`InvalidProfileConfiguration`).

A profile is seeded by `seed <file>` or `homefile .fx/settings.json <file>` /
`homefile .xfx/settings.json <file>`, each optionally behind `fx:` or `xfx:`.
The rule is the one xfx now copies (`src/config.rs`, `ProfileRefusal`): the
file is not one JSON object within 64 KiB; `workspaces` or an entry in it is
not an object (any entry: a scan cannot know the run's workspace key, so it
over-reports); a `model` in the object or an entry is not a string fx's
`validateModel` takes, unless `skill_match_fuzzy` stands before it.
Prints one line per seeding scenario: the verdict per seed, no path.

usage: seed_scan.py <scenarios dir>
"""
import json
import os
import re
import sys

MAX = 64 * 1024


def model_refused(obj):
    if "skill_match_fuzzy" in obj or "model" not in obj:
        return False
    model = obj["model"]
    if not isinstance(model, str):
        return True
    raw = model.encode()
    return (
        len(raw) == 0
        or len(raw) > 1024
        or model.strip(" \t\r\n") != model
        or any(b < 0x20 or b == 0x7F for b in raw)
    )


def verdict(path):
    data = open(path, "rb").read()
    if len(data) > MAX:
        return "refused: too large"
    try:
        obj = json.loads(data.decode("utf-8"))
    except (UnicodeDecodeError, ValueError):
        return "refused: not JSON"
    if not isinstance(obj, dict):
        return "refused: not an object"
    if model_refused(obj):
        return "refused: model"
    ws = obj.get("workspaces")
    if ws is not None:
        if not isinstance(ws, dict):
            return "refused: workspaces not an object"
        for value in ws.values():
            if not isinstance(value, dict):
                return "refused: an entry not an object"
            if model_refused(value):
                return "refused: an entry's model"
    return "starts"


def main():
    scen = sys.argv[1]
    seed_re = re.compile(
        r"^(?:(?:fx|xfx):\s*)?(?:seed\s+(\S+)|homefile\s+\.x?fx/settings\.json\s+(\S+))\s*$"
    )
    caps = sorted(f for f in os.listdir(scen) if f.endswith(".cap"))
    seeding = refused = 0
    for cap in caps:
        seeds = []
        for line in open(os.path.join(scen, cap)):
            m = seed_re.match(line.strip())
            if m:
                seeds.append(m.group(1) or m.group(2))
        if not seeds:
            continue
        seeding += 1
        verdicts = [f"{s}={verdict(os.path.join(scen, s))}" for s in seeds]
        if any("refused" in v for v in verdicts):
            refused += 1
        print(f"{cap[:-4]}: {' '.join(verdicts)}")
    print(f"== {len(caps)} scenarios, {seeding} seed a profile, {refused} seed one fx refuses")


main()
