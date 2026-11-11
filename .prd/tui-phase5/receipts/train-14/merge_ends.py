"""The join of main into train 13, checked from both ends, file by file
# Train 14: train 13's join-main/merge_ends.py, copied; what changed for this train, if anything, is said below this line.
(train 12's join-main/merge_ends.py, with the residual and the masked
reading added for a file that disagrees).

usage: python3 -I -B merge_ends.py <repo> <base> <ours> <theirs> <merged>

For every file both sides changed against the merge base: the lines
ours -> merged removes and adds must be exactly the lines base -> theirs
removes and adds, and theirs -> merged exactly base -> ours (as multisets,
`difflib` line opcodes). A file one side alone changed is that side's bytes
(checked too). For a file that disagrees, two more readings: the residual
-- every line by which the two multisets differ, with where each of the
four sides holds that text and the code unit it sits in (the nearest `fn` / a module constant /
`def` / `[[...]]` header and `id = ` above it) -- and the same check with every `:N` / `:N-M` masked on
all four sides (a merge whose only difference from each side's edits is a
reference number both sides moved reads true there). Prints one line a
file and the count that disagree, unmasked and masked.
"""
import collections
import difflib
import re
import subprocess
import sys

repo, base, ours, theirs, merged = sys.argv[1:6]
NUMS = re.compile(r":\d+(?:-\d+)?")
UNIT = re.compile(r"^\s*(?:pub(?:\([a-z]+\))?\s+)?(?:fn|def)\s+\w+|^[A-Z][A-Z0-9_]* = |^\[\[\w+\]\]|^id = ")


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


def masked(lines):
    return [NUMS.sub(":N", x) for x in lines] if lines is not None else None


def unit_of(lines, line):
    """The nearest code-unit header at or above `line` (1-based) in `lines`."""
    found = []
    for i in range(line - 1, -1, -1):
        if UNIT.match(lines[i]):
            found.append(lines[i].strip()[:70])
            if not lines[i].startswith("id = "):
                break
    return " / ".join(reversed(found)) or "(top)"


both = sorted(changed(base, ours) & changed(base, theirs))
only_ours = sorted(changed(base, ours) - changed(base, theirs))
only_theirs = sorted(changed(base, theirs) - changed(base, ours))
bad = bad_masked = 0
for path in both:
    b, o, t, m = (text(c, path) for c in (base, ours, theirs, merged))
    one = edits(o, m) == edits(b, t)
    two = edits(t, m) == edits(b, o)
    ok = one and two
    bad += not ok
    print("%-4s both  %s (ours->merged = base->theirs: %s; theirs->merged = base->ours: %s)" % ("ok" if ok else "BAD", path, one, two))
    if ok:
        continue
    for name, x, y in (("ours->merged minus base->theirs", edits(o, m), edits(b, t)), ("theirs->merged minus base->ours", edits(t, m), edits(b, o))):
        extra, missing = x - y, y - x
        print("     %s: %d line(s) more, %d fewer" % (name, sum(extra.values()), sum(missing.values())))
        for line in sorted(extra) + sorted(missing):
            sign = "more " if line in extra else "fewer"
            where = []
            for label, lines in (("base", b), ("ours", o), ("theirs", t), ("merged", m)):
                hits = [i + 1 for i, z in enumerate(lines or []) if z == line[1:]]
                if hits:
                    where.append("%s :%s" % (label, ",".join(map(str, hits))))
            first = [i + 1 for i, z in enumerate(b) if z == line[1:]] or [i + 1 for i, z in enumerate(m) if z == line[1:]]
            where = "; ".join(where) + ("; in %s" % unit_of(b if line[1:] in b else m, first[0]) if first else "")
            print("       %s %s  [%s]" % (sign, line[:120], where))
    bm, om, tm, mm = (masked(x) for x in (b, o, t, m))
    one = edits(om, mm) == edits(bm, tm)
    two = edits(tm, mm) == edits(bm, om)
    bad_masked += not (one and two)
    print("     numbers masked: ours->merged = base->theirs: %s; theirs->merged = base->ours: %s" % (one, two))
for path, side in [(p, ours) for p in only_ours] + [(p, theirs) for p in only_theirs]:
    ok = text(merged, path) == text(side, path)
    bad += not ok
    bad_masked += not ok
    if not ok:
        print("BAD  one side's file differs from that side: %s" % path)
print("files both sides changed: %d; one side only: %d (ours %d, theirs %d); disagreeing: %d; disagreeing with numbers masked: %d"
      % (len(both), len(only_ours) + len(only_theirs), len(only_ours), len(only_theirs), bad, bad_masked))
