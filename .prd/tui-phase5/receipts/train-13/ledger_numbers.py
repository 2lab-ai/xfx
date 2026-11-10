#!/usr/bin/env python3
# Train 13: train 12's ledger_numbers.py, unchanged but for this line.
"""Train 12: the ledger entry token_check.py hands back "read by hand".

token_check.py (train 7's, byte for byte) masks a reference's numbers only
for a path under `src|scripts|tests|docs`; a `.prd/` path keeps its numbers.
So an entry both sides of a merge re-pointed, one of them in a `.prd/`
receipt, reads "both sides changed it differently". This reads such an
entry in the merge base, ours, theirs, the merge and the head with every
`:N` / `:N-M` masked, whatever the path, and says whether all five are the
same entry once masked (numbers alone) -- and which numbers each holds.

    python3 -I -B ledger_numbers.py <repo> <merge> <head> <table>:<id>...
"""
import re
import subprocess
import sys
import tomllib

NUMS = re.compile(r":\d+(?:-\d+)?")
LEDGER = "scripts/fxcap/parity-ledger.toml"


def git(repo, *a):
    return subprocess.run(["git", "-C", repo, *a], check=True, capture_output=True, text=True).stdout


def entry(repo, commit, table, ident):
    found = [e for e in tomllib.loads(git(repo, "show", f"{commit}:{LEDGER}")).get(table, []) if e.get("id") == ident]
    assert len(found) == 1, (commit, table, ident, len(found))
    return found[0]


def masked(x):
    if isinstance(x, dict):
        return {k: masked(v) for k, v in x.items()}
    if isinstance(x, list):
        return [masked(v) for v in x]
    return NUMS.sub(":N", x) if isinstance(x, str) else x


def numbers(x):
    if isinstance(x, dict):
        return [n for v in x.values() for n in numbers(v)]
    if isinstance(x, list):
        return [n for v in x for n in numbers(v)]
    return [m.group(0) for m in NUMS.finditer(x)] if isinstance(x, str) else []


repo, merge, head = sys.argv[1:4]
ours, theirs = git(repo, "rev-parse", f"{merge}^1", f"{merge}^2").split()
base = git(repo, "merge-base", ours, theirs).strip()
bad = 0
for spec in sys.argv[4:]:
    table, ident = spec.split(":", 1)
    sides = [("base", base), ("ours", ours), ("theirs", theirs), ("merge", merge), ("head", head)]
    es = {name: entry(repo, c, table, ident) for name, c in sides}
    same = all(masked(es[n]) == masked(es["base"]) for n in es)
    bad += not same
    print(f"{table} {ident}: the five the same once every :N is masked: {same}")
    for name, c in sides:
        print(f"  {name:6} {c[:8]}: {' '.join(numbers(es[name]))}")
sys.exit(1 if bad else 0)
