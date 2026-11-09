#!/usr/bin/env python3
"""Train 10: a docs/parity.md table row both sides of a merge changed, merged
token by token. The row is split into runs of non-blanks and runs of blanks
(so a code span's double space is kept); each side's edits against the merge
base (`difflib` opcodes) must touch base ranges that do not overlap, and do
not meet at the same point -- else it stops and says so, to be read by hand.
The merged row is the base with both sides' edits applied.

    python3 -I -B rowmerge.py <repo> <base> <ours> <theirs> <file> <row-prefix>   # prints the row
    python3 -I -B rowmerge.py --apply <repo> <base> <ours> <theirs> <file> <row-prefix>  # puts it in the worktree's file

With --apply the worktree's file must hold the row inside one conflict block
(`<<<<<<<` .. `>>>>>>>`) whose two sides are ours' and theirs' rows; the block
is replaced by the merged row.
"""
import difflib
import re
import subprocess
import sys

apply = sys.argv[1] == "--apply"
args = sys.argv[2:] if apply else sys.argv[1:]
repo, base, ours, theirs, path, prefix = args


def row(commit):
    text = subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], check=True, capture_output=True, text=True).stdout
    hits = [l for l in text.split("\n") if l.startswith(prefix)]
    if len(hits) != 1:
        sys.exit(f"{commit}: {len(hits)} rows start with {prefix!r}")
    return hits[0]


def toks(s):
    return re.findall(r"\S+|\s+", s)


b, o, t = (row(c) for c in (base, ours, theirs))
tb = toks(b)
edits = []
for name, side in (("ours", toks(o)), ("theirs", toks(t))):
    for op, a1, a2, b1, b2 in difflib.SequenceMatcher(None, tb, side, autojunk=False).get_opcodes():
        if op != "equal":
            edits.append((a1, a2, name, side[b1:b2]))
edits.sort(key=lambda e: (e[0], e[1]))
for (a1, a2, n1, _), (c1, c2, n2, _) in zip(edits, edits[1:]):
    if n1 != n2 and (c1 < a2 or (c1 == a1 and c1 == c2) or (a1 == a2 and a1 == c1)):
        sys.exit(f"edits meet: {n1} base[{a1}:{a2}] and {n2} base[{c1}:{c2}] -- read by hand")
merged, at = [], 0
for a1, a2, name, new in edits:
    merged += tb[at:a1] + new
    at = a2
merged += tb[at:]
text = "".join(merged)
for name, a1, a2 in ((e[2], e[0], e[1]) for e in edits):
    print(f"# {name}: base tokens [{a1}:{a2}] replaced", file=sys.stderr)
if not apply:
    print(text)
    sys.exit(0)
work = open(f"{repo}/{path}", encoding="utf-8").read().split("\n")
starts = [i for i, l in enumerate(work) if l.startswith("<<<<<<< ")]
done = 0
for s in starts:
    mid = next(i for i in range(s, len(work)) if work[i] == "=======")
    end = next(i for i in range(mid, len(work)) if work[i].startswith(">>>>>>> "))
    if work[s + 1:mid] == [o] and work[mid + 1:end] == [t]:
        work[s:end + 1] = [text]
        done += 1
        break
if done != 1:
    sys.exit("no conflict block holding exactly ours' and theirs' rows")
open(f"{repo}/{path}", "w", encoding="utf-8").write("\n".join(work))
print(f"# {path}: the block of {prefix!r} replaced by the merged row ({len(text)} chars)", file=sys.stderr)
