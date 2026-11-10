#!/usr/bin/env python3
"""Train 13, join check 3 (train 9's shared.py, its functions found instead
of listed): the functions of a file that two sides both changed, and
whether the head keeps each side's change.

Every side's hunks against the merge base (`git diff -U0 <base> <side>`)
are given to the function they fall in: the nearest `fn` line above the
hunk in the base, and the function's body runs to the first line after it
at its own indentation that closes it (`}` alone). A function two sides
both changed is then read on the base, on each side and at the head, and
printed train 9's way, its text's sha256 (12 hex digits) per commit; and
checked from both ends, as filecheck.py checks a file: the line edits that
take one side's function to the head's are exactly the edits that took the
base's to the other side's (`difflib` opcodes, compared as what went and
what came). A function a side added or deleted whole is not a shared one.
Every function only one side changed is listed too, with whether the head
holds that side's text of it byte for byte; and each side's hunks that fall
in no function (an insertion between two, a new test) are counted.

    python3 -I -B shared.py <repo> <base> <head> <file> <side>=<commit>...
"""
import difflib
import hashlib
import itertools
import re
import subprocess
import sys

FN = re.compile(r"^(\s*)(?:pub(?:\([a-z]+\))? )?(?:async )?(?:unsafe )?fn ([A-Za-z0-9_]+)\b")


def show(repo, commit, path):
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], check=True, capture_output=True,
                          text=True).stdout.split("\n")


def functions(lines):
    """{(name, indent, ordinal): (start, end)} 1-based, inclusive; a name may
    repeat (a test helper in two modules), so the ordinal keeps them apart."""
    out, seen = {}, {}
    for i, line in enumerate(lines):
        m = FN.match(line)
        if not m:
            continue
        ind = m.group(1)
        close = ind + "}"
        end = next((j for j in range(i + 1, len(lines)) if lines[j] == close), None)
        if end is None or line.rstrip().endswith(";"):
            continue
        key = (m.group(2), len(ind))
        seen[key] = seen.get(key, 0) + 1
        out[key + (seen[key],)] = (i + 1, end + 1)
    return out


def touched(repo, base, side, path, fns, loose=None):
    """The base functions a side's hunks fall in; each hunk in none (an
    insertion between functions, a new test) is counted in `loose`."""
    out = set()
    spans = sorted(fns.items(), key=lambda kv: kv[1][0])
    for line in subprocess.run(["git", "-C", repo, "diff", "-U0", base, side, "--", path], check=True,
                               capture_output=True, text=True).stdout.split("\n"):
        m = re.match(r"@@ -(\d+)(?:,(\d+))? ", line)
        if not m:
            continue
        a, n = int(m.group(1)), int(m.group(2) or 1)
        lo, hi = (a, a + n - 1) if n else (a, a)  # an insertion after `a`: the function holding line a
        inner = [k for k, (s, e) in spans if s <= lo and hi <= e]
        if inner:
            out.add(max(inner, key=lambda k: fns[k][0]))  # the innermost
        elif loose is not None:
            loose.append((lo, hi, n))
    return out


def body(lines, key):
    fns = functions(lines)
    if key not in fns:
        return None
    s, e = fns[key]
    return lines[s - 1:e]


def ops(a, b):
    return [(tuple(a[i1:i2]), tuple(b[j1:j2])) for op, i1, i2, j1, j2 in
            difflib.SequenceMatcher(None, a, b, autojunk=False).get_opcodes() if op != "equal"]


def digest(lines):
    return hashlib.sha256("\n".join(lines).encode()).hexdigest()[:12] if lines is not None else "-"


def main():
    repo, base, head, path = sys.argv[1:5]
    sides = [a.split("=", 1) for a in sys.argv[5:]]
    b_lines = show(repo, base, path)
    fns = functions(b_lines)
    loose = {name: [] for name, _ in sides}
    by = {name: touched(repo, base, commit, path, fns, loose[name]) for name, commit in sides}
    h_lines = show(repo, head, path)
    bad = shared = 0
    for name, commit in sides:
        print(f"# {name}: {len(by[name])} functions of {path} changed against {base[:8]}; "
              f"{len(loose[name])} hunks in no function (base lines "
              f"{', '.join(f'{a}' + (f'-{b}' if b != a else '') + ('' if n else ' (an insertion after)') for a, b, n in loose[name][:12])}"
              f"{', ...' if len(loose[name]) > 12 else ''})")
        others = set().union(*[by[o] for o, _ in sides if o != name])
        for key in sorted(by[name], key=lambda k: fns[k][0]):
            b0, s1, h = body(b_lines, key), body(show(repo, commit, path), key), body(h_lines, key)
            alone = key not in others
            kept = alone and s1 == h  # both None: the side deleted it, and the head has none
            bad += alone and not kept
            print(f"    `fn {key[0]}` (base :{fns[key][0]}): base {digest(b0)} | {name} {digest(s1)} | head {digest(h)}"
                  f"{'; changed by this side alone, the head holds its text: ' + str(kept) if alone else '; changed by another side too (below)'}")
    for (n1, c1), (n2, c2) in itertools.combinations(sides, 2):
        both = sorted(by[n1] & by[n2], key=lambda k: fns[k][0])
        print(f"## {n1} x {n2}: {len(both)} functions both changed")
        for key in both:
            shared += 1
            b0 = body(b_lines, key)
            s1, s2, h = (body(show(repo, c, path), key) for c in (c1, c2, head))
            if None in (s1, s2, h):
                print(f"  {key[0]}: not on every side (renamed or deleted) -- read by hand")
                bad += 1
                continue
            one = ops(s1, h) == ops(b0, s2)
            two = ops(s2, h) == ops(b0, s1)
            bad += not (one and two)
            print(f"  `fn {key[0]}` (base {path}:{fns[key][0]}): {n1} -> head is {n2}'s edits: {one}; "
                  f"{n2} -> head is {n1}'s edits: {two} ({len(ops(b0, s1))} and {len(ops(b0, s2))} edits)")
            print(f"      base {digest(b0)} | {n1} {digest(s1)} | {n2} {digest(s2)} | head {digest(h)}")
    print(f"# {shared} shared functions; {bad} not kept both ways")
    sys.exit(1 if bad else 0)


main()
