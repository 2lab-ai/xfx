#!/usr/bin/env python3
"""Train 10: a row rowmerge.py merged, checked from both ends: the edits that
take ours' row to the merged one are exactly the edits that took the base's to
theirs', and the edits that take theirs' to the merged one exactly the edits
that took the base's to ours' (`difflib` opcodes over runs of non-blanks and
of blanks, compared as (what went, what came)).

    python3 -I -B rowcheck.py <repo> <base> <ours> <theirs> <merged|-> <file> <row-prefix>

`-` for merged reads the worktree's file.
"""
import difflib
import re
import subprocess
import sys

repo, base, ours, theirs, merged, path, prefix = sys.argv[1:]


def text(c):
    if c == "-":
        return open(f"{repo}/{path}", encoding="utf-8").read()
    return subprocess.run(["git", "-C", repo, "show", f"{c}:{path}"], check=True, capture_output=True, text=True).stdout


def row(c):
    hits = [l for l in text(c).split("\n") if l.startswith(prefix)]
    assert len(hits) == 1, (c, len(hits))
    return hits[0]


def toks(s):
    return re.findall(r"\S+|\s+", s)


def ops(a, b):
    ta, tb = toks(a), toks(b)
    return [("".join(ta[a1:a2]), "".join(tb[b1:b2]))
            for op, a1, a2, b1, b2 in difflib.SequenceMatcher(None, ta, tb, autojunk=False).get_opcodes() if op != "equal"]


b, o, t, m = row(base), row(ours), row(theirs), row(merged)
one, two = ops(o, m) == ops(b, t), ops(t, m) == ops(b, o)
print(f"{path} {prefix!r}: base {len(b)}, ours {len(o)}, theirs {len(t)}, merged {len(m)} chars; "
      f"ours -> merged is theirs' edits: {one}; theirs -> merged is ours' edits: {two} "
      f"({len(ops(b, o))} and {len(ops(b, t))} edits)")
sys.exit(0 if one and two else 1)
