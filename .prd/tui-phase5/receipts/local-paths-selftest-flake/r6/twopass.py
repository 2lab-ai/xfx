#!/usr/bin/env python3
"""twopass.py <git> <tree> <n> <threads|default>: run the check's two git greps n times, compare per-file counts."""
import subprocess, sys, collections, re
git, tree, n, th = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
src = open(tree + "/scripts/check-no-local-paths.sh").read()
# the patterns, built as the check builds them
esc = "\x1b"
control = esc + r"\[[0-9;:?]*[@-~]"
start = "(^|[^A-Za-z0-9._~+-]|" + control + ")"
user = "[A-Za-z0-9_][A-Za-z0-9._-]*"
end = "([^A-Za-z0-9._-]|$)"
pats = [start + "/User[s]/" + user + end, start + "/hom[e]/" + user + end, "/tmp/claud[e]-[0-9]+",
        "(^|[^A-Za-z0-9._~+/-]|" + control + ")claud[e]-[0-9]+/", "/var/folder[s]/"]
args = []
for p in pats: args += ["-e", p]
base = [git, "-c", "core.fsmonitor=false", "-c", "grep.column=false", "-c", "grep.fullName=false", "-c", "color.grep=never",
        "-c", "submodule.recurse=false"]
if th != "default": base += ["-c", "grep.threads=" + th]
base += ["-C", tree, "--work-tree=.", "grep", "-a", "--no-color"]
bad = 0
for i in range(n):
    a = subprocess.run(base + ["-n", "-z", "-E"] + args + ["--"], capture_output=True)
    b = subprocess.run(base + ["-c", "-z", "-E"] + args + ["--"], capture_output=True)
    lines = collections.OrderedDict()
    for rec in a.stdout.split(b"\n"):
        if not rec: continue
        path = rec.split(b"\0", 1)[0]
        lines[path] = lines.get(path, 0) + 1
    counts = collections.OrderedDict()
    for rec in b.stdout.split(b"\n"):
        if not rec: continue
        p, c = rec.split(b"\0", 1)
        counts[p] = int(c)
    if lines != counts or a.returncode > 1 or b.returncode > 1 or a.stderr or b.stderr:
        bad += 1
        diff = [(p.decode(errors="replace"), lines.get(p), counts.get(p)) for p in set(lines) | set(counts) if lines.get(p) != counts.get(p)]
        print(f"iter {i}: DISAGREE {diff[:5]} rc={a.returncode},{b.returncode}", flush=True)
print(f"{git} threads={th}: {bad} disagreements in {n}; files {len(counts)}, lines {sum(counts.values())}")
