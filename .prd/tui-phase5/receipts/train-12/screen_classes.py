#!/usr/bin/env python3
"""Train 11 (train 10's screen_classes.py, its classes this train's): the listed lines of a screen-moves file (a snap whose screen moved beyond
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
    ("the catalog's wall-clock outage: model-column-idle, model-parked, model-reask (reruns.txt)",
     ("model-column-idle", "model-parked", "model-reask")),
    ("SUBMIT-CARD-PREVIEW: the card's preview (p5-submit-preview-*, p5-transcript-submit*)",
     ("p5-submit-preview", "p5-transcript-submit")),
    ("PASTE-GAP: the quarantine's sentences and the plane's rows (paste*, p5-input-paste*, approvals*, p5-input-approval)",
     ("paste", "p5-input-approval")),
    ("on main, not on the unit's base: AT-HEIGHT's @ list, MODEL-FUZZY's ranks, QUESTIONS-20's pages",
     ("p5-input-at", "p5-input-model", "model-anchor", "p5-input-question q6", "workspace-index", "filepicker")),
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
