#!/usr/bin/env python3
"""Train 14 (new), join checks 3 and 4: named functions read whole at named
commits, each printed as its text's sha256 (12 hex digits; train 9's form)
or `absent`, one row per function, one column per commit; then, for each
function and each pair of adjacent commits in a `--diff a..b` list, the
lines that differ (`difflib`, `-` the first's, `+` the second's).

A function is found as shared.py finds it: its `fn <name>` line (the first
at the indentation given, `ind` spaces) and its body to the first `}` alone
at that indentation.

    python3 -I -B functions.py <repo> <path>:<name>[:<ind>]... -- <label>=<commit>... [--diff <a>..<b>]...
"""
import difflib
import hashlib
import re
import subprocess
import sys

args = sys.argv[1:]
repo = args.pop(0)
cut = args.index("--")
fns = [a.split(":") for a in args[:cut]]
rest = args[cut + 1:]
diffs = [rest[i + 1].split("..") for i, a in enumerate(rest) if a == "--diff"]
commits = [a.split("=", 1) for i, a in enumerate(rest) if a != "--diff" and (i == 0 or rest[i - 1] != "--diff")]


def body(commit, path, name, ind):
    r = subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], capture_output=True, text=True)
    if r.returncode:
        return None
    lines = r.stdout.split("\n")
    pat = re.compile(r"^" + " " * ind + r"(?:pub(?:\([a-z]+\))? )?fn " + re.escape(name) + r"\b")
    for i, line in enumerate(lines):
        if pat.match(line) and not line.rstrip().endswith(";"):
            end = next((j for j in range(i + 1, len(lines)) if lines[j] == " " * ind + "}"), None)
            return lines[i:end + 1] if end is not None else None
    return None


texts = {}
print("| function | " + " | ".join(f"{label} `{c[:8]}`" for label, c in commits) + " |")
print("|---|" + "---|" * len(commits))
for spec in fns:
    path, name = spec[0], spec[1]
    ind = int(spec[2]) if len(spec) > 2 else 4
    row = []
    for label, c in commits:
        b = body(c, path, name, ind)
        texts[(name, label)] = b
        row.append("absent" if b is None else hashlib.sha256("\n".join(b).encode()).hexdigest()[:12] + f" ({len(b)})")
    print(f"| `{path}` `{name}` | " + " | ".join(row) + " |")
for a, b in diffs:
    for spec in fns:
        name = spec[1]
        x, y = texts.get((name, a)), texts.get((name, b))
        if x is None or y is None or x == y:
            continue
        print(f"\n## `{name}`, {a} -> {b}")
        for line in difflib.unified_diff(x, y, lineterm="", n=0):
            if line.startswith(("---", "+++", "@@")):
                continue
            print("    " + line[:160])
