#!/usr/bin/env python3
"""Train 11, round 2: the catalog asks in asks.txt (this folder's), read for
the sweep's one drop, `model-column-idle` (the ledger's `catalog-window`
cause; the fake Gateway's catalog down for its first 9000 ms, `modelsdown
9000`).

1. Every run's asks per binary, and the gap between a binary's first and
   second ask: the second is the column's, when the scenario types `/model `
   after its `wait 6` (opus's reading), not a timer's retry.
2. The rule that reading gives: a binary lists the rows at `col-down` when its
   first ask is at or past 9000 ms less that gap, so a pair passes when both
   binaries' first asks fall on the same side of it; checked against each of
   the 8 runs' verdicts, and against each binary's own answer (a 200 logged
   or not).
3. The first asks' medians per binary, for `model-column-idle` (4 runs each)
   and over both wall-clock scenarios (`model-column-idle`, `model-parked`;
   8 runs each), as opus counted them.

    python3 -I -B first_asks.py <asks.txt>
"""
import re
import statistics
import sys

OUTAGE = 9000
lines = open(sys.argv[1], encoding="utf-8").read().splitlines()
LINE = re.compile(r"^(\S+): (\S+) (\S+) (\S+); fx asked at ([^;]*); answered ([^;]*); xfx asked at ([^;]*); answered (.*)$")
runs = []
for line in lines:
    m = LINE.match(line)
    if not m:
        continue
    name, scenario, label, verdict, fx_asked, fx_ans, xfx_asked, xfx_ans = m.groups()
    nums = lambda s: [int(x) for x in re.findall(r"(\d+) ms", s)]  # noqa: E731
    runs.append({"run": name, "scenario": scenario, "verdict": verdict,
                 "fx": nums(fx_asked), "xfx": nums(xfx_asked),
                 "fx_200": " 200 " in f" {fx_ans} ", "xfx_200": " 200 " in f" {xfx_ans} "})
col = [r for r in runs if r["scenario"] == "model-column-idle"]
gaps = [r[b][1] - r[b][0] for r in col for b in ("fx", "xfx") if len(r[b]) > 1]
gap = round(statistics.median(gaps))
threshold = OUTAGE - gap
print(f"# model-column-idle: {len(col)} runs; gaps between a binary's first and second ask: "
      f"{min(gaps)}-{max(gaps)} ms ({len(gaps)} gaps), median {gap} ms; threshold {OUTAGE} - {gap} = {threshold} ms")
hit_pairs = hit_sides = 0
for r in col:
    side = {b: r[b][0] >= threshold for b in ("fx", "xfx")}
    predicted = "pass" if side["fx"] == side["xfx"] else "fail"
    actual = "pass" if r["verdict"] in ("identical", "normalized-identical") else "fail"
    rows = {b: r[f"{b}_200"] for b in ("fx", "xfx")}
    hit_pairs += predicted == actual
    hit_sides += sum(side[b] == rows[b] for b in ("fx", "xfx"))
    print(f"{r['run']}: fx first {r['fx'][0]} ms ({'past' if side['fx'] else 'before'}), "
          f"xfx first {r['xfx'][0]} ms ({'past' if side['xfx'] else 'before'}); predicted {predicted}, "
          f"verdict {r['verdict']}; rows (a 200 logged): fx {rows['fx']}, xfx {rows['xfx']}")
print(f"# the rule: {hit_pairs} of {len(col)} verdicts; each binary's rows: {hit_sides} of {2 * len(col)}")
for scope, rs in (("model-column-idle", col), ("both scenarios", runs)):
    for b, where in (("xfx", "main"), ("xfx", "head"), ("fx", "main"), ("fx", "head")):
        firsts = sorted(r[b][0] for r in rs if r["run"].startswith(where))
        print(f"# first asks, {scope}, {b} in the {where} runs ({len(firsts)}): {firsts}; "
              f"median {statistics.median(firsts) / 1000:.2f} s")
