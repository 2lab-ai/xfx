"""Which commits edited the receipts the change edits (not adds), and whether
the coverage of /status survives the deleted scenario.

usage: python3 -I edited_receipts.py <checkout at 1b15dad9> <base rev> <head scoreboard.json>
"""
import collections
import json
import os
import subprocess
import sys

repo, base, sb = sys.argv[1:4]
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
import ci_sweep  # noqa: E402


def git(*a):
    return subprocess.run(["git", "-C", repo] + list(a), capture_output=True, check=True).stdout.decode()


edited = [p for p in git("diff", "--name-only", "--no-renames", "--diff-filter=M", "-z", base, "HEAD").split("\0")
          if p and ci_sweep.RECEIPT_PATH.match(p)]
by_set = collections.Counter()
subjects = {}
for p in edited:
    shas = git("log", "--format=%h", "--no-merges", base + "..HEAD", "--", p).split()
    for h in shas:
        subjects.setdefault(h, git("log", "-1", "--format=%s", h).strip())
    by_set[" ".join(sorted(shas))] += 1
print("receipts the change edits (not adds): %d" % len(edited))
print("by the set of commits that edited each:")
for k, n in by_set.most_common():
    print("  %3d  %s" % (n, k))
print("those commits:")
for h, s in sorted(subjects.items()):
    print("  %s %s" % (h, s[:110]))
# LOCAL-PATHS's own commits: on its branch, merged into train 12 at 55d993a7
# (second parent), and not on the train before it (first parent).
lp = set(git("log", "--format=%h", "55d993a7^1..55d993a7^2").split())
mine = {h for h in subjects if h in lp}
only = sum(n for k, n in by_set.items() if set(k.split()) <= lp)
print("of those, LOCAL-PATHS's (55d993a7^1..55d993a7^2): %s" % " ".join(sorted(mine)))
print("receipts edited by LOCAL-PATHS's commits and no other: %d of %d" % (only, len(edited)))

d = json.load(open(sb))
for s in d["surfaces"]:
    if s["surface"] == "/status":
        print("\n/status, its scenarios at CI's head: %d: %s" % (len(s["scenarios"]), " ".join(s["scenarios"])))
        rest = [n for n in s["scenarios"] if n != "p5-panels-badsettings"]
        print("without the deleted one: %d" % len(rest))
print("coverage at CI's head: covered %s of %s surfaces" % (d["coverage"]["covered"], d["coverage"]["surfaces"]))
