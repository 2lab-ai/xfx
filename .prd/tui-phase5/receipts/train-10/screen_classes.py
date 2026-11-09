#!/usr/bin/env python3
"""The listed lines of a screen-moves file (a snap whose screen moved beyond
clock-like or run-local text, on xfx alone or at other rows than fx's), each
put in a class by its text, the classes counted, every line of "other" shown.

    python3 -I -B screen_classes.py <screen-moves-*.txt>...
"""
import collections
import re
import sys

CLASSES = [
    ("a saved session's id (resume ids, /status's Session row)", ("ume-id", "--resume", "Session  ")),
    ("the build id on /status", ("Build             debug",)),
    ("the spinner's glyph in front of Compacting (a frame's timing)", ("Compacting",)),
    ("AT-HEIGHT: the @ list's height and rows", ("filepicker", "p5-input-at", "workspace-index")),
    ("MODEL-FUZZY: the ranked rows", ("p5-input-model", "model-anchor")),
    ("PASTE-AFTER-END / QUESTIONS-20 (on main, not on the unit)", ("p5-input-paste paste-enter", "p5-input-question q6")),
    ("model-parked menu-loading: the catalog's arrival before the snap (reruns.txt)", ("model-parked",)),
    ("the checkout path in an MCP server's command row (fx's moved too)", ("p5-mcp-verbs",)),
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
