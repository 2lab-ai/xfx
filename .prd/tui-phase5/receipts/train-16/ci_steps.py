#!/usr/bin/env python3
"""Train 16 (new), for ci-steps.sh: a workflow file on stdin, each step out
as `name | if | run` (a multi-line `run: |` as its lines joined by `; `)."""
import sys

lines = sys.stdin.read().split("\n")
out, cur = [], None
i = 0
while i < len(lines):
    s = lines[i].strip()
    if s.startswith("- name:"):
        if cur:
            out.append(cur)
        cur = {"name": s[len("- name:"):].strip(), "if": "", "run": ""}
    elif cur is not None and s.startswith("if:"):
        cur["if"] = s[3:].strip()
    elif cur is not None and s.startswith("run:"):
        rest = s[4:].strip()
        if rest == "|":
            ind = len(lines[i + 1]) - len(lines[i + 1].lstrip())
            body = []
            i += 1
            while i < len(lines) and (lines[i].strip() == "" or len(lines[i]) - len(lines[i].lstrip()) >= ind):
                if lines[i].strip():
                    body.append(lines[i].strip())
                i += 1
            cur["run"] = "; ".join(body)
            continue
        cur["run"] = rest
    i += 1
if cur:
    out.append(cur)
for st in out:
    print(" | ".join((st["name"], st["if"], st["run"])))
