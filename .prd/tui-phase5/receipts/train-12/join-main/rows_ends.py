#!/usr/bin/env python3
"""docs/parity.md's table rows across the join of main, from both ends.

    python3 -I -B rows_ends.py <repo> <base> <ours> <theirs> <merged>

A row is keyed by its first cell (rowsmerge.py's key; a key may hold
several rows, compared as a list). For each key: which sides changed it
against the base, and whether the merge holds the changed side's rows (the
base's when neither did). A key both sides changed is printed as such; the
join had no conflict, so there should be none.
"""
import subprocess
import sys

repo, base, ours, theirs, merged = sys.argv[1:6]
PATH = "docs/parity.md"


def rows(commit):
    text = subprocess.run(["git", "-C", repo, "show", "%s:%s" % (commit, PATH)], check=True, capture_output=True, text=True).stdout
    out = {}
    for line in text.split("\n"):
        cells = line.split("|")
        if line.startswith("|") and len(cells) > 2:
            out.setdefault(cells[1].strip(), []).append(line)
    return out


b, o, t, m = rows(base), rows(ours), rows(theirs), rows(merged)
both = bad = 0
counts = {"ours": 0, "theirs": 0}
for k in sorted(set(b) | set(o) | set(t) | set(m)):
    co, ct = o.get(k) != b.get(k), t.get(k) != b.get(k)
    if co and ct:
        both += 1
        ok = o.get(k) == t.get(k) == m.get(k)
        print("BOTH %s %s" % ("same" if ok else "DIFFER", k[:80]))
        bad += not ok
        continue
    want = o.get(k) if co else t.get(k) if ct else b.get(k)
    if co or ct:
        counts["ours" if co else "theirs"] += 1
        print("%s %s %s" % ("ours" if co else "theirs", "ok" if m.get(k) == want else "WRONG", k[:80]))
    bad += m.get(k) != want
print("# %d keys; changed by ours %d, by theirs %d, by both %d; %d not as the changed side left them"
      % (len(set(b) | set(o) | set(t) | set(m)), counts["ours"], counts["theirs"], both, bad))
