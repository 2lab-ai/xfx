#!/usr/bin/env python3
"""Train 14 (new), pathcheck.sh's helper for a commit that is not a merge:
splits the staged added lines (`git diff --cached -U0`) into those whose
every run-name or uid-tail match (shapes.py's two shapes beyond the brief's)
already stands in HEAD's text of the same file -- a line rewritten around a
name the file held before, which the commit does not add -- and the rest.
Prints the rest on stdout, for shapes.py to count, and each carried line on
stderr as its file and the matched text's shape (never the text). The
brief's own pattern is not read here: pathcheck.sh gates every staged line
on it, with no exception.

    python3 -I -B carried.py <worktree>
"""
import re
import subprocess
import sys

repo = sys.argv[1]
SHAPES = [("run-name", re.compile(r"fxs[w]\.[A-Za-z0-9_]{6}|fxca[p]\.(?!XXXXXX)[A-Za-z0-9]{6}")),
          ("uid-tail", re.compile(r"claud[e]-[0-9]+|(?<![^\s'\"`(<>])-[0-9]+/"))]
diff = subprocess.run(["git", "-C", repo, "diff", "--cached", "-U0"], capture_output=True, text=True, check=True,
                      errors="replace").stdout
path, heads = None, {}
for line in diff.split("\n"):
    if line.startswith("+++ "):
        path = line[6:] if line.startswith("+++ b/") else None
        continue
    if not line.startswith("+") or path is None:
        continue
    text = line[1:]
    hits = [(name, m.group(0)) for name, rx in SHAPES for m in rx.finditer(text)]
    if not hits:
        print(line)
        continue
    if path not in heads:
        r = subprocess.run(["git", "-C", repo, "show", f"HEAD:{path}"], capture_output=True, text=True, errors="replace")
        heads[path] = r.stdout if r.returncode == 0 else ""
    if all(found in heads[path] for _, found in hits):
        print(f"    carried ({', '.join(sorted({n for n, _ in hits}))}): {path}", file=sys.stderr)
    else:
        print(line)
