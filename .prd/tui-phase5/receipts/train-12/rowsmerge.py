#!/usr/bin/env python3
"""Train 12: train 10's rowmerge.py for a conflict block of several table
rows. Every conflict block of docs/parity.md (git's plain style, `<<<<<<<`
.. `=======` .. `>>>>>>>`) must hold, on each side, whole table rows with
the same first cells in the same order; each row is merged token by token
against the merge base's row of that first cell, by rowmerge.py's rule: the
row is split into runs of non-blanks and runs of blanks, each side's edits
against the base (`difflib` opcodes) must touch base ranges that do not
overlap and do not meet at one point -- else it stops, changing nothing, to
be read by hand. A row only one side changed comes out as that side's row.

    python3 -I -B rowsmerge.py <repo> <base> <ours> <theirs> <file>

<repo> is the worktree; the merged rows replace the blocks in its file.
"""
import difflib
import re
import subprocess
import sys

repo, base, ours, theirs, path = sys.argv[1:]


def toks(s):
    return re.findall(r"\S+|\s+", s)


def key(row):
    cells = row.split("|")
    return cells[1].strip() if row.startswith("|") and len(cells) > 2 else None


def rows(commit):
    text = subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], check=True, capture_output=True, text=True).stdout
    out = {}
    for line in text.split("\n"):
        k = key(line)
        if k is not None:
            out.setdefault(k, []).append(line)
    return out


B, O, T = rows(base), rows(ours), rows(theirs)


def one(k, o, t):
    for name, side in (("base", B), ("ours", O), ("theirs", T)):
        if len(side.get(k, [])) != 1:
            sys.exit(f"{name}: {len(side.get(k, []))} rows keyed {k!r} -- read by hand")
    b = B[k][0]
    if (o, t) != (O[k][0], T[k][0]):
        sys.exit(f"{k!r}: the block's rows are not ours' and theirs' -- read by hand")
    tb = toks(b)
    edits = []
    for name, side in (("ours", toks(o)), ("theirs", toks(t))):
        for op, a1, a2, b1, b2 in difflib.SequenceMatcher(None, tb, side, autojunk=False).get_opcodes():
            if op != "equal":
                edits.append((a1, a2, name, side[b1:b2]))
    edits.sort(key=lambda e: (e[0], e[1]))
    for (a1, a2, n1, _), (c1, c2, n2, _) in zip(edits, edits[1:]):
        if n1 != n2 and (c1 < a2 or (c1 == a1 and c1 == c2) or (a1 == a2 and a1 == c1)):
            sys.exit(f"{k!r}: edits meet: {n1} base[{a1}:{a2}] and {n2} base[{c1}:{c2}] -- read by hand")
    merged, at = [], 0
    for a1, a2, _, new in edits:
        merged += tb[at:a1] + new
        at = a2
    merged += tb[at:]
    by = sorted({e[2] for e in edits})
    print(f"# {path}: row {k!r}: {len([e for e in edits if e[2] == 'ours'])} edits of ours, "
          f"{len([e for e in edits if e[2] == 'theirs'])} of theirs ({' and '.join(by) or 'neither'})", file=sys.stderr)
    return "".join(merged)


work = open(f"{repo}/{path}", encoding="utf-8").read().split("\n")
out, i, blocks = [], 0, 0
while i < len(work):
    if not work[i].startswith("<<<<<<< "):
        out.append(work[i])
        i += 1
        continue
    mid = next(j for j in range(i, len(work)) if work[j] == "=======")
    end = next(j for j in range(mid, len(work)) if work[j].startswith(">>>>>>> "))
    o, t = work[i + 1:mid], work[mid + 1:end]
    if [key(r) for r in o] != [key(r) for r in t] or None in [key(r) for r in o]:
        sys.exit(f"{path}:{i + 1}: the block's sides are not the same table rows -- read by hand")
    out.extend(one(key(a), a, b) for a, b in zip(o, t))
    blocks += 1
    i = end + 1
open(f"{repo}/{path}", "w", encoding="utf-8").write("\n".join(out))
print(f"# {path}: {blocks} blocks resolved", file=sys.stderr)
