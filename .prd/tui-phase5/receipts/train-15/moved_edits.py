#!/usr/bin/env python3
"""Train 15 (new), join check 1: an edit made in a function one side moved.
RULE-CONFIRM-KEYS moved `Shell::decide`'s translation into a function of its
own, `Shell::approval_action`; the decoder line edited that translation
inside `decide`. Read as functions.py reads them (`fn` line to the first `}`
alone at its indentation), this checks that the decoder's edits of `decide`
(<base-a>..<side-a>) are exactly the edits that take the moved function on
the other side (<side-b>) to the merge (<merge>): the same lines out and the
same lines in, as multisets (`difflib` opcodes over lines). And that
`decide` at the merge is <side-b>'s byte for byte, so nothing of the
decoder's stayed behind in it.

    python3 -I -B moved_edits.py <repo> <path> <from-fn>@<base-a>..<side-a> <to-fn>@<side-b>..<merge>
"""
import collections
import difflib
import hashlib
import re
import subprocess
import sys

repo, path, a, b = sys.argv[1:5]


def body(commit, name):
    lines = subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], capture_output=True, text=True,
                           check=True).stdout.split("\n")
    rx = re.compile(r"^(\s*)(?:pub(?:\([a-z]+\))? )?fn " + re.escape(name) + r"\b")
    i = next(k for k, l in enumerate(lines) if rx.match(l))
    ind = rx.match(lines[i]).group(1)
    j = next(k for k in range(i + 1, len(lines)) if lines[k] == ind + "}")
    return lines[i:j + 1]


def edits(x, y):
    out, came = collections.Counter(), collections.Counter()
    for op, a1, a2, b1, b2 in difflib.SequenceMatcher(None, x, y, autojunk=False).get_opcodes():
        if op != "equal":
            out.update(x[a1:a2])
            came.update(y[b1:b2])
    return out, came


def h(lines):
    return hashlib.sha256("\n".join(lines).encode()).hexdigest()[:12]


fa, ra = a.split("@")
fb, rb = b.split("@")
a0, a1 = ra.split("..")
b0, b1 = rb.split("..")
ea = edits(body(a0, fa), body(a1, fa))
eb = edits(body(b0, fb), body(b1, fb))
print(f"`{fa}` {a0[:8]}..{a1[:8]}: {sum(ea[0].values())} lines out, {sum(ea[1].values())} in")
print(f"`{fb}` {b0[:8]}..{b1[:8]}: {sum(eb[0].values())} lines out, {sum(eb[1].values())} in")
same = ea == eb
print(f"the same lines out and in: {same}")
for name, side in (("out", 0), ("in", 1)):
    for line, n in sorted((ea[side] - eb[side]).items()):
        print(f"    only in {fa}'s edits ({name}): {line.strip()[:120]} x{n}")
    for line, n in sorted((eb[side] - ea[side]).items()):
        print(f"    only in {fb}'s edits ({name}): {line.strip()[:120]} x{n}")
d0, d1 = h(body(b0, fa)), h(body(b1, fa))
print(f"`{fa}` at {b0[:8]} {d0}, at {b1[:8]} {d1}: the merge's is {b0[:8]}'s byte for byte: {d0 == d1}")
sys.exit(0 if same and d0 == d1 else 1)
