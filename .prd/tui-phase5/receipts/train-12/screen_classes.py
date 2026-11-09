#!/usr/bin/env python3
"""Train 12 (trains 10's and 11's screen_classes.py, its classes this train's): the listed lines of a screen-moves file (a snap whose screen moved beyond
clock-like or run-local text, on xfx alone or at other rows than fx's), each
put in a class by its text, the classes counted, every line of "other" shown.

    python3 -I -B screen_classes.py <screen-moves-*.txt>...
"""
import collections
import re
import sys

CLASSES = [
    ("a saved session's id (resume ids, /status's Session row)", ("ume-id", "--resume", "Session  ", "· not saved yet", "` of this saved session")),
    ("LOCAL-PATHS: a path of the run written as its placeholder on the head (main's `<local-path>` row, the head's `<scratch___>` / `<work___>` / `<tmp___>` fill)",
     ("<local-path>", "<scratch", "<work", "<tmp", "<home", "______")),
    ("the build id: /status's Build row, and the banner where a `.full` snap keeps it below row 1", ("Build             debug", "v0.1.0-df46777 [debug]")),
    ("a turn's retry, caught at a fixed moment (`Recovered answer.` or `recovered · attempt 3`; p5-lifecycle-retryok rerun in reruns.txt)", ("recovered · attempt", "RECOVERED-ANSWER", "Recovered answer")),
    ("an MCP server's connection state, caught at a fixed moment (`retrying` or `connecting`; p5-mcp-menu-new rerun in reruns.txt)", ("retrying                profile", "connecting              profile")),
    ("xfx's /stats row: `ansi_bytes`, the bytes its frames wrote (4367 on main, 4391 at the head; the pairs' verdicts unmoved)", ("debounced_resizes=0, footer_updates=0",)),
    ("SWEEP-WALLCLOCK-F: the eight catalog scenarios, the outage now ended at a scenario line",
     ("model-column-idle", "model-column-reask", "model-reask", "model-parked", "p5-input-model-fail",
      "p5-stop-in-catalog-wait", "model-floor", "p5-effort-fork-effort-down")),
]
for path in sys.argv[1:]:
    lines = [l.strip() for l in open(path, encoding="utf-8") if re.match(r"^    (fx|xfx) ", l)]
    count, other = collections.Counter(), []
    for l in lines:
        name = next((n for n, keys in CLASSES if any(k in l for k in keys)), None)
        if name is None:
            other.append(l)
        count[name or "other"] += 1
    print(f"== {path.rsplit('/', 1)[-1]}: {len(lines)} listed lines")
    for name, n in count.most_common():
        print(f"  {n}  {name}")
    for l in other:
        print(f"    other: {l[:200]}")
