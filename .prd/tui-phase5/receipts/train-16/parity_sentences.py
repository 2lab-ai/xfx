#!/usr/bin/env python3
"""Train 16, round 2 (gpt6's NTH 2): parity-join.sh's last section, moved out
of its `python3 -c '...'`, whose quoting broke it (a SyntaxError in round 1's
parity-join.txt). Each sentence the join commit edited, as docs/parity.md
(on stdin) holds it: from the words it starts with to the end of its cell
(` | `) or line, at most 1100 characters; or NOT FOUND.

    git show <commit>:docs/parity.md | python3 -I -B parity_sentences.py
"""
import sys

STARTS = (
    "fx's composer also completes a typed argument from its menu before Enter, and so does",
    "**The draft's keys are fx v0.0.16's**",
    "behind the meta prefix too (`ESC ESC [106;5u`",
    "The meta-prefixed ones are C-j too:",
    "refuses it as not an xfx command when it takes none.",
)
text = sys.stdin.read()
missing = 0
for start in STARTS:
    i = text.find(start)
    if i < 0:
        missing += 1
        print(f"- NOT FOUND: {start}\n")
        continue
    ends = [j for j in (text.find(" | ", i), text.find("\n", i)) if j >= 0]
    end = min(ends + [i + 1100])
    print(f"- (at {text.count(chr(10), 0, i) + 1}) {text[i:min(end, i + 1100)]}\n")
print(f"# {len(STARTS) - missing} of {len(STARTS)} found")
sys.exit(1 if missing else 0)
