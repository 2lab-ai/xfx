#!/usr/bin/env python3
"""The join of main into train 13: docs/parity.md's table rows and the
ledger's entries, from both ends (train 12's join-main/rows_ends.py, with the
ledger read entry by entry and a key both sides changed read with its numbers
masked).

    python3 -I -B rows_ends.py <repo> <base> <ours> <theirs> <merged>

docs/parity.md: a row is keyed by its first cell (rowsmerge.py's key; a key
may hold several rows, compared as a list). The ledger: an entry is keyed by
its table and `id` (`cause:<id>`), its lines from its `[[...]]` header to the
next (the comment lines above a header go with the entry before it). For each
key: which sides changed it against the base, and whether the merge holds the
changed side's text (the base's when neither did). A key both sides changed
is BOTH, read from both ends as merge_ends.py reads a file: `same` when the
two sides and the merge are equal; `both-ends` when ours -> merged holds
exactly base -> theirs' line edits and theirs -> merged exactly base ->
ours'; `numbers` when that holds once every `:N` / `:N-M` is masked, with
each number both sides moved listed as base / ours / theirs -> merged (the
join's rule for one both moved: ours, then the re-point); else DIFFER.
"""
import collections
import difflib
import re
import subprocess
import sys

repo, base, ours, theirs, merged = sys.argv[1:6]
NUMS = re.compile(r":\d+(?:-\d+)?")


def show(commit, path):
    return subprocess.run(["git", "-C", repo, "show", "%s:%s" % (commit, path)], check=True, capture_output=True, text=True).stdout


def rows(commit):
    out = {}
    for line in show(commit, "docs/parity.md").split("\n"):
        cells = line.split("|")
        if line.startswith("|") and len(cells) > 2:
            out.setdefault(cells[1].strip(), []).append(line)
    return out


def entries(commit):
    out, key, kind = {}, None, None
    for line in show(commit, "scripts/fxcap/parity-ledger.toml").split("\n"):
        m = re.match(r"^\[\[(\w+)\]\]$", line)
        if m:
            kind, key = m.group(1), None
            continue
        m = re.match(r'^(?:id|name) = "([^"]+)"$', line)
        if kind and key is None and m:
            key = "%s:%s" % (kind, m.group(1))
            out[key] = [line]
            continue
        if key:
            out[key].append(line)
    return out


def masked(v):
    return None if v is None else [NUMS.sub(":N", x) for x in v]


def edits(a, b):
    out = collections.Counter()
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, a or [], b or [], autojunk=False).get_opcodes():
        if tag != "equal":
            out.update("-" + x for x in (a or [])[i1:i2])
            out.update("+" + y for y in (b or [])[j1:j2])
    return out


def ends(b, o, t, m):
    return edits(o, m) == edits(b, t) and edits(t, m) == edits(b, o)


def aligned(b, x):
    """base line index -> x's line index, for lines equal once masked."""
    out = {}
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, masked(b), masked(x), autojunk=False).get_opcodes():
        if tag == "equal":
            out.update(zip(range(i1, i2), range(j1, j2)))
    return out


def moved(b, o, t, m):
    ao, at, am = aligned(b, o), aligned(b, t), aligned(b, m)
    out = []
    for i, line in enumerate(b):
        if i in ao and i in at and i in am:
            for y, x, z, w in zip(NUMS.findall(line), NUMS.findall(o[ao[i]]), NUMS.findall(t[at[i]]), NUMS.findall(m[am[i]])):
                if not (x == y == z):
                    out.append("%s / %s / %s -> %s" % (y, x, z, w))
    return out


def check(title, read):
    b, o, t, m = (read(c) for c in (base, ours, theirs, merged))
    keys = sorted(set(b) | set(o) | set(t) | set(m))
    bad = 0
    counts = {"ours": 0, "theirs": 0, "both": 0}
    print("## %s" % title)
    for k in keys:
        co, ct = o.get(k) != b.get(k), t.get(k) != b.get(k)
        if co and ct:
            counts["both"] += 1
            if o.get(k) == t.get(k) == m.get(k):
                verdict = "same"
            elif ends(b.get(k), o.get(k), t.get(k), m.get(k)):
                verdict = "both-ends"
            elif ends(*(masked(x.get(k)) for x in (b, o, t, m))):
                verdict = "numbers (base / ours / theirs -> merged: %s)" % ", ".join(moved(b[k], o[k], t[k], m[k]))
            else:
                verdict = "DIFFER"
                bad += 1
            print("BOTH %s  %s" % (k, verdict))
            continue
        want = o.get(k) if co else t.get(k) if ct else b.get(k)
        if co or ct:
            counts["ours" if co else "theirs"] += 1
            print("%s %s %s" % ("ours  " if co else "theirs", "ok" if m.get(k) == want else "WRONG", k[:80]))
        bad += m.get(k) != want
    print("# %d keys; changed by ours %d, by theirs %d, by both %d; %d not as the changed side left them (or DIFFER)"
          % (len(keys), counts["ours"], counts["theirs"], counts["both"], bad))
    return bad


bad = check("docs/parity.md, rows by first cell", rows) + check("scripts/fxcap/parity-ledger.toml, entries by table and id", entries)
sys.exit(1 if bad else 0)
