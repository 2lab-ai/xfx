#!/usr/bin/env python3
"""Train 16 (new): the second re-point's four (repoint.py's EXTRA), checked
at <commit> directly, since repointed.py reads a bare `:N`'s file from the
ref itself and has none for one: for each, the citing file holds the
before-text with the new number exactly once, the old number nowhere after
that text, and the cited file's lines at the new number start with the text
the table names (each ` / `-separated line, stripped).

    python3 -I -B extra_check.py <repo> <commit>
"""
import os
import subprocess
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
import repoint  # noqa: E402

repo, commit = sys.argv[1:3]
# The file each EXTRA ref names: its own path, or for a bare `:N` the path of
# the ref it continues on its line (read at <commit>).
CITED = {
    ("docs/parity.md", "src/gateway/mod.rs:1005-1006"): "src/gateway/mod.rs",
    ("scripts/fxcap/capfile.py", ":15785-15787"): "src/tui/shell.rs",
    ("src/tui/shell.rs", ":3535"): "src/tui/worker.rs",
    ("src/tui/shell.rs", ":6418"): "src/tui/shell.rs",
}


def show(path):
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], capture_output=True, text=True,
                          check=True).stdout


bad = 0
for (citing, before, spelled), (new, text, _) in repoint.EXTRA.items():
    body = show(citing)
    want = before + repoint.numbers(spelled, new)
    once = body.count(want) == 1 and body.count(before + spelled) == 0
    path = CITED[(citing, spelled)]
    lines = show(path).split("\n")
    first, last = (int(x) for x in (new.split("-") + [new])[:2])
    got = [lines[i - 1].strip() for i in range(first, last + 1)]
    expect = [t.strip() for t in text.split(" / ")]
    holds = all(g.startswith(e[:40].rstrip(".")) for g, e in zip(got, expect)) and len(expect) <= len(got)
    ok = once and holds
    bad += not ok
    print(f"{'ok ' if ok else 'BAD'} {citing} {spelled} -> {new}: the citing text with the new number once and the old "
          f"nowhere: {once}; {path}:{new} holds {got!r}")
print(f"# {len(repoint.EXTRA)} checked at {commit[:8]}: {len(repoint.EXTRA) - bad} ok, {bad} bad")
sys.exit(1 if bad else 0)
