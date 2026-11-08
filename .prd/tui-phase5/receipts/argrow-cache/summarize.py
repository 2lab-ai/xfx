"""Summarizes the probe's PROBE3 lines (probe.txt): per (arguments, key, case, clone), round 1 / round 2.

    python3 -I summarize.py probe.txt > probe-summary.txt
"""
import re, sys, collections
rows = collections.defaultdict(list)
order = []
for line in open(sys.argv[1]):
    m = re.match(r"round=(\d) clone=(\S+) load=(\S+) exit=(\d+) PROBE3 (.*)", line)
    if not m:
        continue
    rnd, clone, load, rc, rest = m.groups()
    kv = dict(p.split("=", 1) for p in rest.split())
    key = (kv["detail"], kv["key"], kv["case"], clone)
    if key not in rows:
        order.append(key)
    rows[key].append((rnd, load, rc, kv))
fields = ["ctrlo_open_ms", "ctrlo_key_ms", "ctrlo_join_ms", "ctrlo_theme_ms", "ctrlo_resize_ms", "ctrlo_pageup_ms", "detail_ms", "bridge_ms", "peak_rss_mib"]
print("detail key case clone | " + " | ".join(fields) + " | exits | loads")
for key in order:
    vals = rows[key]
    cells = []
    for f in fields:
        cells.append(" / ".join(v[3][f] for v in vals))
    print(" ".join(key) + " | " + " | ".join(cells) + " | " + ",".join(v[2] for v in vals) + " | " + ",".join(v[1] for v in vals))
