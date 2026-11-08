#!/usr/bin/env python3
"""Train 8, join check 2: run.sh's output as tables -- GRID-ROW-COST's
summarize.py for FRAME and CTRLO (its MEM left out), each figure `round 1 /
round 2` -- and the ratios the brief asks for: the head against GRID-ROW-COST
alone and against main, step by step and round by round (a step of the head
more than 1.2x GRID-ROW-COST alone's is a finding).

    python3 -I -B summarize.py probe.txt > probe-summary.txt
"""
import collections
import re
import sys

FIELD = re.compile(r"(\w+)=(\S+)")
frame = collections.defaultdict(list)
ctrlo = collections.defaultdict(list)
order = []
for line in open(sys.argv[1], encoding="utf-8"):
    kind = line.split(" ", 1)[0]
    fields = dict(FIELD.findall(line))
    clone = fields.get("clone", "")
    if clone and clone not in order:
        order.append(clone)
    if kind == "FRAME" and "frame_median_ms" in fields:
        frame[(fields["case"], fields["rows"], clone)].append(fields)
    elif kind == "CTRLO" and "ctrlo_open_ms" in fields:
        ctrlo[(fields["case"], clone)].append(fields)


def both(runs, key, digits=1):
    return " / ".join(f"{float(run[key]):.{digits}f}" for run in runs)


print("## FRAME: ms, round 1 / round 2; GRID-ROW-COST's bar: frame median <= 16 ms with the long rows left in place")
print("| case | long calls | build | frame median | frame p90 | apply median | event median | event worst | first placement (worst) | peak RSS MiB | load (1 min) |")
print("|---|---|---|---|---|---|---|---|---|---|---|")
for (case, rows, clone), runs in sorted(frame.items(), key=lambda kv: (kv[0][0], kv[0][1], order.index(kv[0][2]))):
    print(f"| {case} | {rows} | {clone} | {both(runs, 'frame_median_ms')} | {both(runs, 'frame_p90_ms')} | "
          f"{both(runs, 'apply_median_ms')} | {both(runs, 'event_median_ms')} | {both(runs, 'event_worst_ms')} | "
          f"{both(runs, 'start_worst_ms')} | {both(runs, 'peak_rss_mib', 0)} | {both(runs, 'load', 1)} |")
print()
print("## CTRLO: train 6's Ctrl-O probe as ARGROW-CACHE extended it, XFX_PROBE_DETAIL=1, ms, round 1 / round 2")
keys = ["ctrlo_open_ms", "ctrlo_join_ms", "ctrlo_theme_ms", "ctrlo_resize_ms", "start_ms", "detail_ms", "peak_rss_mib", "load"]
print("| case | build | " + " | ".join(k.replace("_ms", "").replace("ctrlo_", "") for k in keys) + " |")
print("|---|---|" + "---|" * len(keys))
for (case, clone), runs in sorted(ctrlo.items(), key=lambda kv: (kv[0][0], order.index(kv[0][1]))):
    print(f"| {case} | {clone} | " + " | ".join(both(runs, k, 0 if k == 'peak_rss_mib' else (2 if k == 'load' else 1)) for k in keys) + " |")
print()
print("## head / grid and head / main, each step, round by round (the same round's runs, which ran case by case side by side)")
FRAME_KEYS = ["frame_median_ms", "frame_p90_ms", "apply_median_ms", "event_median_ms", "event_worst_ms", "start_worst_ms", "peak_rss_mib"]
worst = {}
for other in ("grid", "main"):
    print(f"### head / {other}")
    print("| measure | " + " | ".join(FRAME_KEYS) + " |")
    print("|---|" + "---|" * len(FRAME_KEYS))
    for (case, rows, clone), runs in sorted(frame.items(), key=lambda kv: (kv[0][0], kv[0][1])):
        if clone != "head" or (case, rows, other) not in frame:
            continue
        theirs = frame[(case, rows, other)]
        cells = []
        for k in FRAME_KEYS:
            ratios = [float(h[k]) / float(t[k]) if float(t[k]) else float("nan") for h, t in zip(runs, theirs)]
            cells.append(" / ".join(f"{r:.2f}" for r in ratios))
            if other == "grid":
                worst[(f"FRAME {case}:{rows}", k)] = max(ratios)
        print(f"| FRAME {case}:{rows} | " + " | ".join(cells) + " |")
    for (case, clone), runs in sorted(ctrlo.items()):
        if clone != "head" or (case, other) not in ctrlo:
            continue
        theirs = ctrlo[(case, other)]
        cells = []
        for k in keys[:-1]:
            ratios = [float(h[k]) / float(t[k]) if float(t[k]) else float("nan") for h, t in zip(runs, theirs)]
            cells.append(f"{k} " + " / ".join(f"{r:.2f}" for r in ratios))
            if other == "grid":
                worst[(f"CTRLO {case}", k)] = max(ratios)
        print(f"| CTRLO {case} | " + "; ".join(cells) + " |")
    print()
past = sorted((v, k) for k, v in worst.items() if v > 1.2)
print(f"## head / grid past 1.2x on any step, any round: {len(past)}")
for v, (where, k) in past:
    print(f"    {where} {k}: {v:.2f}")
