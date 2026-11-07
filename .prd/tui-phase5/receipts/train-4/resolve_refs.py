#!/usr/bin/env python3
"""Resolve a merge's ledger conflicts that are `path:line` references alone.

    python3 -I -B resolve_refs.py <worktree> <ledger> [--apply]

Run with the merge in progress (conflict markers in <ledger>). For each
conflict hunk, the two sides must differ in `path:line` numbers only: the
same text once every number after `<path>:` is masked. Ours (HEAD) is taken,
and each of its references into a file is mapped from HEAD's copy of that
file to the working tree's (the merge's own result) through
`git diff -U0 HEAD -- <path>`. Printed for each: the reference, both sides'
numbers, the new number, and the line ours cited at HEAD against the line at
the new number in the working tree, with the two lines on either side; a
reference whose five lines are not the same text fails the run. With
--apply the resolved text replaces the hunk.
"""
import re
import subprocess
import sys

REF = re.compile(r"\b((?:src|scripts|tests|docs)/[A-Za-z0-9_./-]+\.(?:rs|sh|py|md|toml|ts|json)):(\d+)(?:-(\d+))?(?!\d|-\d|@)")
HUNK = re.compile(r"^<<<<<<< [^\n]*\n(.*?)^=======\n(.*?)^>>>>>>> [^\n]*\n", re.S | re.M)


def git(tree, *args):
    return subprocess.run(["git", "-C", tree, *args], check=True, capture_output=True, text=True).stdout


def spans(tree, path):
    found = []
    for line in git(tree, "diff", "-U0", "HEAD", "--", path).splitlines():
        m = re.match(r"@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", line)
        if m:
            a, b, c, d = m.groups()
            found.append((int(a), int(b) if b is not None else 1, int(c), int(d) if d is not None else 1))
    return found


def mapped(line, hunks):
    offset = 0
    for old_start, old_len, _new_start, new_len in hunks:
        if old_len == 0:
            if line > old_start:
                offset += new_len
            continue
        if old_start <= line < old_start + old_len:
            return None
        if line >= old_start + old_len:
            offset += new_len - old_len
    return line + offset


def main():
    tree, ledger = sys.argv[1], sys.argv[2]
    apply = "--apply" in sys.argv[3:]
    text = open(f"{tree}/{ledger}", encoding="utf-8").read()
    bad = 0
    count = 0

    def resolve(m):
        nonlocal bad, count
        ours, theirs = m.group(1), m.group(2)
        mask = lambda s: REF.sub(lambda r: r.group(1) + ":N", s)
        if mask(ours) != mask(theirs):
            bad += 1
            print("NOT A REFERENCE-ONLY CONFLICT:\n" + ours + "=======\n" + theirs)
            return m.group(0)
        theirs_refs = [(r.group(1), r.group(2)) for r in REF.finditer(theirs)]

        def repoint(r):
            nonlocal bad, count
            path, first = r.group(1), int(r.group(2))
            if r.group(3):
                bad += 1
                print(f"RANGE not handled: {r.group(0)}")
                return r.group(0)
            new = mapped(first, spans(tree, path))
            other = [n for p, n in theirs_refs if p == path]
            head = git(tree, "show", f"HEAD:{path}").splitlines()
            work = open(f"{tree}/{path}", encoding="utf-8").read().splitlines()
            same = new is not None and all(
                head[first - 1 + k] == work[new - 1 + k] for k in (-2, -1, 0, 1, 2))
            bad += not same
            count += 1
            print(f"{path}:{first} (ours) / :{','.join(other)} (theirs) -> :{new}  "
                  f"{'SAME' if same else 'DIFFERENT'} (5 lines): {work[new - 1].strip()[:80] if new else None!r}")
            return f"{path}:{new}"

        return REF.sub(repoint, ours)

    out = HUNK.sub(resolve, text)
    print(f"{count} references re-pointed; {bad} problems; markers left: {out.count('<<<<<<<')}")
    if apply and not bad:
        open(f"{tree}/{ledger}", "w", encoding="utf-8").write(out)
        print("applied")
    sys.exit(1 if bad else 0)


main()
