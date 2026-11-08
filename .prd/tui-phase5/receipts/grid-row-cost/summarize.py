#!/usr/bin/env python3
"""run.sh's output as tables: FRAME (one row a case, long calls and clone, each
figure `round 1 / round 2`), MEM (the stages, live heap and peak RSS, round 1)
and CTRLO (round 1 / round 2).

    python3 -I -B summarize.py probe.txt > probe-summary.txt
"""
import collections
import re
import sys

FIELD = re.compile(r"(\w+)=(\S+)")
frame = collections.defaultdict(list)
mem = collections.defaultdict(dict)
ctrlo = collections.defaultdict(list)
order = []
for line in open(sys.argv[1], encoding="utf-8"):
    kind = line.split(" ", 1)[0]
    fields = dict(FIELD.findall(line))
    clone = fields.get("clone", "").replace("probe-", "")
    if clone and clone not in order:
        order.append(clone)
    if kind == "FRAME" and "frame_median_ms" in fields:
        frame[(fields["case"], fields["rows"], clone)].append(fields)
    elif kind == "MEM" and fields.get("round") == "1":
        mem[(fields["case"], clone)][fields["stage"]] = (fields["live_mib"], fields["peak_rss_mib"])
        mem[(fields["case"], clone)]["load"] = fields["load"]
    elif kind == "CTRLO" and "ctrlo_open_ms" in fields:
        ctrlo[(fields["case"], clone)].append(fields)


def both(runs, key, digits=1):
    return " / ".join(f"{float(run[key]):.{digits}f}" for run in runs)


print("## FRAME: ms, round 1 / round 2; bar: frame median <= 16 ms with the long rows left in place")
print("| case | long calls | build | frame median | frame p90 | apply median | event median | event worst | first placement (worst) | peak RSS MiB | load (1 min) |")
print("|---|---|---|---|---|---|---|---|---|---|---|")
for (case, rows, clone), runs in sorted(frame.items(), key=lambda kv: (kv[0][0], kv[0][1], order.index(kv[0][2]))):
    print(f"| {case} | {rows} | {clone} | {both(runs, 'frame_median_ms')} | {both(runs, 'frame_p90_ms')} | "
          f"{both(runs, 'apply_median_ms')} | {both(runs, 'event_median_ms')} | {both(runs, 'event_worst_ms')} | "
          f"{both(runs, 'start_worst_ms')} | {both(runs, 'peak_rss_mib', 0)} | {both(runs, 'load', 1)} |")
print()
print("## MEM: live heap MiB / peak RSS MiB after each stage, round 1")
stages = ["start", "detail", "rebuilds", "ctrlo_open", "ctrlo_join", "ctrlo_theme", "ctrlo_resize", "ctrlo_closed_again"]
print("| case | build | " + " | ".join(stages) + " | load (1 min) |")
print("|---|---|" + "---|" * (len(stages) + 1))
for (case, clone), got in sorted(mem.items(), key=lambda kv: (kv[0][0], order.index(kv[0][1]))):
    print(f"| {case} | {clone} | " + " | ".join(f"{got[s][0]} / {got[s][1]}" if s in got else "" for s in stages) + f" | {got['load']} |")
print()
print("## CTRLO: train 6's Ctrl-O probe, XFX_PROBE_DETAIL=1, ms, round 1 / round 2")
keys = ["ctrlo_open_ms", "ctrlo_join_ms", "ctrlo_theme_ms", "ctrlo_resize_ms", "detail_ms", "peak_rss_mib", "load"]
print("| case | build | " + " | ".join(k.replace("_ms", "").replace("ctrlo_", "") for k in keys) + " |")
print("|---|---|" + "---|" * len(keys))
for (case, clone), runs in sorted(ctrlo.items(), key=lambda kv: (kv[0][0], order.index(kv[0][1]))):
    print(f"| {case} | {clone} | " + " | ".join(both(runs, k, 0 if k == 'peak_rss_mib' else (2 if k == 'load' else 1)) for k in keys) + " |")
