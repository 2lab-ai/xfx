#!/usr/bin/env python3
"""Train 16 (new): the second re-point's four (repoint.py's EXTRA), checked
at <commit> directly, since repointed.py reads a bare `:N`'s file from the
ref itself and has none for one: for each, the citing file holds the
before-text with the new number exactly once, the old number nowhere after
that text, and the cited file's lines at the new number start with the text
the table names (each ` / `-separated line, stripped).

    python3 -I -B extra_check.py <repo> <commit> [--text]

Round 2 (opus's MUST 1), with --text: TEXT's spans too, each checked three
ways at <commit>: the new span once in its file and the old nowhere; each
number of the new span naming the text TEXT_CITED gives (its start and end,
stripped), the same line the base's number names at the base; and each the
base span's number at that place, mapped through
`git diff -U0 <base> <commit>` of the cited file (refs_all.py's `hunks` and
`mapped`, refs_bare.py's MOVED).
"""
import re
import os
import subprocess
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
import repoint  # noqa: E402
from refs_all import hunks, mapped  # noqa: E402

repo, commit = sys.argv[1:3]
TEXT_OLD = "`tests/tui.rs:1930,2191,2766,3113,3236,3928,7648,7722,8515,9216,9412`"
# (citing, old span) -> (cited file, start, end) every number's line holds
TEXT_CITED = {("tests/tui.rs", TEXT_OLD): ("tests/tui.rs", "session.type_bytes(&[", "0x15, 0x04]);")}
# (citing, old span) -> (base, the span as the base holds it)
TEXT_BASE = {("tests/tui.rs", TEXT_OLD): (
    "866747ff", "`tests/tui.rs:1809,2191,2766,3113,3236,3928,7648,7722,8515,9216,9412`")}
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
if "--text" in sys.argv:
    tbad = 0
    for (citing, old), (new, _) in repoint.TEXT.items():
        body = show(citing)
        once = body.count(new) == 1 and body.count(old) == 0
        path, start, end = TEXT_CITED[(citing, old)]
        lines = show(path).split("\n")
        base, base_span = TEXT_BASE[(citing, old)]
        base_body = subprocess.run(["git", "-C", repo, "show", f"{base}:{citing}"], capture_output=True,
                                   text=True, check=True).stdout
        base_lines = subprocess.run(["git", "-C", repo, "show", f"{base}:{path}"], capture_output=True,
                                    text=True, check=True).stdout.split("\n")
        spans = hunks(repo, base, commit, path)
        nums = [int(x) for x in re.findall(r"[:,](\d+)", new)]
        olds = [int(x) for x in re.findall(r"[:,](\d+)", base_span)]
        print(f"TEXT {citing}: the new span once and the old nowhere: {once}; the base span once at {base}: "
              f"{base_body.count(base_span) == 1}")
        for o, n in zip(olds, nums):
            line, was = lines[n - 1].strip(), base_lines[o - 1].strip()
            ok = (line.startswith(start) and line.endswith(end) and mapped(o, spans) == n and line == was)
            tbad += not ok
            print(f"  {'ok ' if ok else 'BAD'} {base}'s :{o} -> :{mapped(o, spans)} mapped, written :{n}; {path}:{n} holds {line!r}, as {base}'s :{o} did: {line == was}")
        tbad += not once or len(olds) != len(nums)
    print(f"# TEXT: {len(repoint.TEXT)} span(s) at {commit[:8]}, {tbad} bad")
    bad += tbad
sys.exit(1 if bad else 0)
