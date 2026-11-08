#!/usr/bin/env python3
"""Reads a macOS `sample` report and prints, from its call graph, every frame
whose symbol names one of this unit's functions: its depth in the tree and its
sample count, in tree order. Paths are not printed (the report's header names
the binary's folder). usage: profile_tree.py <sample report>"""
import re
import sys

lines = open(sys.argv[1]).read().split("\n")
start = next(i for i, l in enumerate(lines) if l.startswith("Call graph:"))
end = next((i for i, l in enumerate(lines) if l.startswith("Total number in stack")), len(lines))
frame = re.compile(r"^([ +!:|]*)(\d+)\s+(\S+)")
keys = ["commit_frame", "commit_document", "emit_front", "append_floating",
        "render_append_recorded", "row_matches", "place_row", "preflight",
        "Expected", "tokenize_row", "seed_primary", "from_shadow",
        "build_frame", "plan", "prepare_front", "cut_for_screen",
        "Shell5apply", "ToolGroup"]
for line in lines[start:end]:
    m = frame.match(line)
    if not m:
        continue
    depth, count, name = len(m.group(1)), int(m.group(2)), m.group(3)
    if any(k in name for k in keys):
        print(f"{depth:4d} {count:6d} {name[:150]}")
