#!/usr/bin/env python3
"""The fx citations this unit added: every code span naming a `.zig` line in the lines added since
714f6283 under the given paths, less those the removed lines already had (a rewritten row keeps
text from before the unit). Comment continuations are joined first, so a span wrapped across
lines is whole. Prints each span in backticks, for `fx_src.sh <fx> --check`.

    python3 -I -B new_cites.py <path>... > <file>
"""
import re
import subprocess
import sys

diff = subprocess.check_output(["git", "diff", "714f6283", "--", *sys.argv[1:]], text=True).split("\n")
plus = "\n".join(line[1:] for line in diff if line.startswith("+") and not line.startswith("+++"))
minus = "\n".join(line[1:] for line in diff if line.startswith("-") and not line.startswith("---"))


def spans(text):
    text = re.sub(r"\n\s*(?://[/!]?|#)\s*", " ", text)
    return [span for span in re.findall(r"`([^`\n]*)`", text) if re.search(r"\.zig:\d", span)]


before = set(spans(minus))
for span in dict.fromkeys(spans(plus)):
    if span not in before:
        print(f"`{span}`")
