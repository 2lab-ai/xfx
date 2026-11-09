#!/usr/bin/env python3
"""Train 11's join tests (train 10's apply_join.py, its list this train's),
put into the tree unchanged: each file's text right before the last line of
its target (the closing brace of the file's tests module), so no line of the file above it
moves -- no reference into these files is moved by them.

    python3 -I -B apply_join.py <repo>
"""
import os
import sys

repo = sys.argv[1]
here = os.path.dirname(os.path.abspath(__file__))
JOINS = [
    ("join_test_paste.rs", "src/tui/event_loop.rs"),
    ("join_test_catalog.rs", "src/tui/event_loop.rs"),
]
for test, target in JOINS:
    path = os.path.join(repo, target)
    text = open(path, encoding="utf-8").read()
    assert text.endswith("    }\n}\n"), f"{target} does not end its tests module"
    body = open(os.path.join(here, test), encoding="utf-8").read()
    open(path, "w", encoding="utf-8").write(text[: -len("}\n")] + body + "}\n")
    print(f"{target}: {test} ({body.count(chr(10))} lines) before its last line")
