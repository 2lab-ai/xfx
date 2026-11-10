"""The join of main into train 12, checked from both ends, file by file.

usage: python3 -I -B merge_ends.py <repo> <base> <ours> <theirs> <merged>
  <merged>  a commit, or `INDEX` for the staged merge

For every file both sides changed against the merge base: the lines
ours -> merged removes and adds must be exactly the lines base -> theirs
removes and adds, and theirs -> merged exactly base -> ours (as multisets,
`difflib` line opcodes). A file one side alone changed is that side's bytes
(checked too). Prints one line a file and the count that disagree.
"""
import collections
import difflib
import subprocess
import sys

repo, base, ours, theirs, merged = sys.argv[1:6]


def git(*a):
    return subprocess.run(["git", "-C", repo, *a], capture_output=True, check=True).stdout


def text(commit, path):
    spec = (":" + path) if commit == "INDEX" else "%s:%s" % (commit, path)
    try:
        return git("show", spec).decode("utf-8", errors="surrogateescape").split("\n")
    except subprocess.CalledProcessError:
        return None


def changed(a, b):
    return set(git("diff", "--name-only", "--no-renames", a, b).decode().split())


def edits(a, b):
    a, b = a or [], b or []
    out = collections.Counter()
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, a, b, autojunk=False).get_opcodes():
        if tag == "equal":
            continue
        for x in a[i1:i2]:
            out["-" + x] += 1
        for y in b[j1:j2]:
            out["+" + y] += 1
    return out


both = sorted(changed(base, ours) & changed(base, theirs))
only_ours = sorted(changed(base, ours) - changed(base, theirs))
only_theirs = sorted(changed(base, theirs) - changed(base, ours))
bad = 0
for path in both:
    b, o, t, m = (text(c, path) for c in (base, ours, theirs, merged))
    one = edits(o, m) == edits(b, t)
    two = edits(t, m) == edits(b, o)
    ok = one and two
    bad += not ok
    print("%-4s both  %s (ours->merged = base->theirs: %s; theirs->merged = base->ours: %s)" % ("ok" if ok else "BAD", path, one, two))
for path, side in [(p, ours) for p in only_ours] + [(p, theirs) for p in only_theirs]:
    ok = text(merged, path) == text(side, path)
    bad += not ok
    if not ok:
        print("BAD  one side's file differs from that side: %s" % path)
print("files both sides changed: %d; one side only: %d (ours %d, theirs %d); disagreeing: %d"
      % (len(both), len(only_ours) + len(only_theirs), len(only_ours), len(only_theirs), bad))
