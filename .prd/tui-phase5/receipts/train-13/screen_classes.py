#!/usr/bin/env python3
"""Train 13 (trains 10's to 12's screen_classes.py, its classes this train's): the listed lines of a screen-moves file (a snap whose screen moved beyond
clock-like or run-local text, on xfx alone or at other rows than fx's), each
put in a class by its text, the classes counted, every line of "other" shown.

    python3 -I -B screen_classes.py <screen-moves-*.txt>...
"""
import collections
import re
import sys

CLASSES = [
    ("a saved session's id (resume ids, /status's Session row)", ("ume-id", "--resume", "Session  ", "· not saved yet", "` of this saved session")),
    ("the build id: /status's Build row, and the banner where a `.full` snap keeps it below row 1", ("Build             debug", "v0.1.0-f53606d [debug]", "v0.1.0-e1e15ef [debug]")),
    ("HISTORY-NOTICES-COPY-FX: the reasoning notice gone (`... so that reasoning was not carried over`)", ("not carried over", "is continuing on", "is continuing with")),
    ("QUESTION-RESUME: a resumed batch's resolutions (`  N) <question>` and its answer) or its `● Asked` row", ("● Asked", ") Pick ", ") Q ")),
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
