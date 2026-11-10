#!/usr/bin/env python3
# Train 14: train 13's refs_content.py, copied; what changed for this train, if anything, is said below this line.
"""Train 13: every reference the parity ledger holds, checked by **content**
-- APPROVAL-VERTICAL-SPELLINGS' refs.py is the model (a ref must name the
line its entry is about), widened from its hand table of 17 refs to every
ref, with the expected text read from history instead of written by hand.

For each citing line of <file> at <head>, `git blame --line-porcelain`
(ignoring the re-point commits given with --ignore, which change numbers
only) names the commit B that wrote it and the line it was there; a line a
merge put together (a parity row merged token by token) is followed into the
parent that holds the same ref and blamed there, until B is not a merge
(or is a merge no parent of which holds those numbers: it wrote them); and
a line a numbers-only edit (a re-point) changed is followed into the parent
while the ref in question stands there with the same numbers, so B is the
commit that wrote that ref's numbers, not one that re-pointed its neighbour
(train 11's re-point re-pointed `src/tui/shell.rs:10480` and left the bare
`:1747` after it). B is the
moment its author wrote the numbers, so the cited file at B, at the number
B's own line holds, is the text the ref is about. The check: the cited
lines at the head's number, and two either side, equal B's (`SAME`); the
cited lines equal but a neighbour not (`SAME-LINES`, read by hand); else
the ref is `WRONG`, with where B's text (and its two either side) stands at
the head, when it stands at exactly one place, or else the head's line most
like it. A ref to one table row (a line starting `|`) is `SAME-ROW` when the
head's line is a row with the same first cell: rows are rewritten in place
(refs.py's `= row`).

What counts as a ref: a repo path (`src/`, `scripts/`, `tests/`, `docs/`,
`.prd/`, or a `build.rs`/`Cargo.toml` at the root) with `:N` or `:N-M`, not
followed by `@` (a pin into another product); a short `<basename>:N` whose
basename names one tracked file (any of the path's extensions but `.zig`:
a short `main.zig:N` is fx's); and a bare `:N` after either, on the same
line, until a ref into another file starts (refs.py's rule). A path that is
neither in the head's tree nor in B's is another product's (fx's `src/...zig`,
say) and is counted, not checked.

A blind spot this cannot close, and refs.py's hand table can: a ref that was
already wrong when B wrote it reads `SAME` (B's text and the head's agree on
the wrong line). Run refs.py beside it for the refs its table holds.

    python3 -I -B refs_content.py <repo> <head> <file> [--ignore <commit>]...

Prints one line per ref that is not `SAME`, the counts per verdict, and
exits 1 when any ref is `WRONG`.
"""
import difflib
import os
import re
import subprocess
import sys

repo = head = citing = None
ignore = []
EXT = r"(?:rs|sh|py|md|toml|ts|json|txt|cap|yml|rb|zig)"
FULL = re.compile(r"(?<![A-Za-z0-9_./-])((?:src|scripts|tests|docs|\.prd)/[A-Za-z0-9_./-]+\." + EXT + r"|build\.rs|Cargo\.toml):(\d+)(?:-(\d+))?(?![\d@]|-\d)")
SHORT = re.compile(r"(?<![A-Za-z0-9_./-])([A-Za-z0-9_-]+\.(?:rs|sh|py|md|toml|ts|json|txt|cap|yml|rb)):(\d+)(?:-(\d+))?(?![\d@]|-\d)")
ANY = re.compile(r"[\w./-]+\.(?:rs|zig|md|toml|ts|py|sh|txt|json|cap|yml|rb):\d+")
BARE = re.compile(r"(?<![\w./:]):(\d+)(?:-(\d+))?(?![\d\w@-])")
NUMS = re.compile(r":\d+(?:-\d+)?")
CTX = 2
_cache = {}


def git(*a):
    return subprocess.run(["git", "-C", repo, *a], check=True, capture_output=True, text=True, errors="replace").stdout


def show(commit, path):
    key = (commit, path)
    if key not in _cache:
        r = subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], capture_output=True, text=True, errors="replace")
        _cache[key] = r.stdout.split("\n") if r.returncode == 0 else None
    return _cache[key]


by_name = {}


def setup(r, h, ign=()):
    """The repo, the head and the commits blame ignores; repoint.py calls it too."""
    global repo, head, ignore
    repo, head, ignore = r, h, list(ign)
    by_name.clear()
    for f in git("ls-tree", "-r", "--name-only", head).splitlines():
        by_name.setdefault(os.path.basename(f), []).append(f)


def refs_of(line):
    """(start, end, path, first, last, kind) of every ref on one line."""
    marks = []
    for m in FULL.finditer(line):
        marks.append((m.start(), m.end(), m.group(1), int(m.group(2)), m.group(3), "path"))
    taken = [(a, b) for a, b, *_ in marks]
    for m in SHORT.finditer(line):
        if any(a <= m.start() < b for a, b in taken):
            continue
        paths = by_name.get(m.group(1), [])
        if len(paths) == 1:
            marks.append((m.start(), m.end(), paths[0], int(m.group(2)), m.group(3), "short"))
    files = sorted(marks)
    others = [m.start() for m in ANY.finditer(line) if not any(a <= m.start() < b for a, b, *_ in files)]
    out = list(files)
    for m in BARE.finditer(line):
        if any(a <= m.start() < b for a, b, *_ in files):
            continue
        before = [f for f in files if f[1] <= m.start()]
        if not before:
            continue
        last = before[-1]
        if any(last[1] <= o < m.start() for o in others):
            continue  # a ref into another file (not this repo's) came between
        out.append((m.start(), m.end(), last[2], int(m.group(1)), m.group(2), "bare"))
    return sorted(out)


def blame(commit, path, n=None):
    """{final line: {commit, path, orig}} of `path` at `commit`, one line or all."""
    rng = ["-L", f"{n},{n}"] if n else []
    cmd = ["blame", "--line-porcelain"] + sum((["--ignore-rev", c] for c in ignore), []) + rng + [commit, "--", path]
    out, cur = {}, None
    for line in git(*cmd).split("\n"):
        m = re.match(r"^([0-9a-f]{40}) (\d+) (\d+)", line)
        if m:
            cur = {"commit": m.group(1), "orig": int(m.group(2)), "final": int(m.group(3))}
        elif line.startswith("filename ") and cur:
            cur["path"] = line[len("filename "):]
            out[cur["final"]] = cur
            cur = None
    return out


_parents = {}


def parents(commit):
    if commit not in _parents:
        _parents[commit] = git("rev-list", "--parents", "-n1", commit).split()[1:]
    return _parents[commit]


def frame(commit, path, n, ref, b=None, depth=0):
    """(B, number, last) for one ref of `path`:`n` at `commit`: the commit
    that wrote the ref's numbers and the numbers it wrote there. The ref is
    first read in B's own line (B's numbers: an ignored re-point left B's
    older ones). A line blamed to a merge commit (a row the merge put
    together from both sides' edits) is followed into the parent whose own
    line holds the same ref -- the same path and B's numbers -- and blamed
    there, until a commit that is not a merge."""
    b = b or blame(commit, path, n)[n]
    then = (show(b["commit"], b["path"]) or [""])[b["orig"] - 1]
    line = show(commit, path)[n - 1]
    found, then_refs = refs_of(line), refs_of(then)
    if NUMS.sub(":N", then) == NUMS.sub(":N", line) and len(then_refs) == len(found):
        i = [r[2:6] for r in found].index(ref[2:6])  # by path, numbers and kind: offsets differ by line
        mine = then_refs[i]
    else:
        mine = None  # the shape changed: B's frame at this number
    if len(parents(b["commit"])) > 1 and depth < 8:
        _, _, rpath, rfirst, rlast, rkind = mine or ref
        for p in parents(b["commit"]):
            text = show(p, b["path"]) or []
            here = [i for i, l in enumerate(text, 1)
                    if any(r[2:5] == (rpath, rfirst, rlast) and r[5] == rkind for r in refs_of(l))]
            if here:
                return frame(p, b["path"], here[0], mine or ref, None, depth + 1)
        # No parent holds these numbers: the merge wrote them (an older
        # train's re-point inside its merge), so the merge is the frame.
    if mine is None:
        return b["commit"], ref[3], int(ref[4]) if ref[4] else None
    f0, l0 = mine[3], int(mine[4]) if mine[4] else None
    # B wrote the line, but did it write this ref's numbers? A numbers-only
    # edit of the line (a re-point) that left this ref as it was is not its
    # writer: follow the line into B's parent while the ref stands there
    # with the same numbers.
    if len(parents(b["commit"])) == 1 and depth < 40:
        p = parents(b["commit"])[0]
        back = parent_line(b["commit"], p, b["path"], b["orig"], then)
        if back is not None:
            prev = (show(p, b["path"]) or [""])[back - 1]
            if any(r[2:6] == mine[2:6] for r in refs_of(prev)):
                return frame(p, b["path"], back, mine, None, depth + 1)
    return b["commit"], f0, l0


def parent_line(commit, parent, path, orig, text):
    """The line of `parent`'s `path` that `commit` turned into its line
    `orig` (`text`) by a numbers-only edit, or None (a line it wrote anew)."""
    out = git("diff", "-U0", parent, commit, "--", path).split("\n")
    for i, h in enumerate(out):
        m = re.match(r"@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", h)
        if not m:
            continue
        a, na, c, nc = int(m.group(1)), int(m.group(2) or 1), int(m.group(3)), int(m.group(4) or 1)
        if not (c <= orig < c + nc):
            continue
        body = []
        for l in out[i + 1:]:
            if l.startswith("@@") or l.startswith("diff "):
                break
            body.append(l)
        old = [l[1:] for l in body if l.startswith("-")]
        hits = [a + j for j, l in enumerate(old) if NUMS.sub(":N", l) == NUMS.sub(":N", text)]
        if len(hits) == 1:
            return hits[0]
        if orig - c < len(old) and a + (orig - c) in hits:
            return a + (orig - c)
        return None
    return None


def row_key(row):
    cells = row.split("|")
    return cells[1].strip() if row.startswith("|") and len(cells) > 2 else None


def window(text, first, last):
    a, b = first - 1 - CTX, (last or first) + CTX
    if text is None or first < 1 or (last or first) > len(text):
        return None, None
    return text[max(a, 0):b], text[first - 1:(last or first)]


def main():
    global citing
    args = sys.argv[1:]
    ign = [args[i + 1] for i, a in enumerate(args) if a == "--ignore"]
    pos = [a for i, a in enumerate(args) if a != "--ignore" and (i == 0 or args[i - 1] != "--ignore")]
    setup(pos[0], pos[1], ign)
    citing = pos[2]
    lines = show(head, citing)
    who = blame(head, citing)
    counts = {"SAME": 0, "SAME-ROW": 0, "SAME-LINES": 0, "WRONG": 0, "other product": 0, "unread": 0}
    print(f"# {citing} at {head[:8]}; blame ignoring {', '.join(c[:8] for c in ignore) or 'nothing'}")
    for n, line in enumerate(lines, 1):
        found = refs_of(line)
        if not found:
            continue
        for ref in found:
            s0, e0, path, first, last, kind = ref
            last_n = int(last) if last else None
            spelled = line[s0:e0]
            if show(head, path) is None and show(who[n]["commit"], path) is None:
                counts["other product"] += 1
                continue
            bc, f0, l0 = frame(head, citing, n, ref, who[n])
            b = {"commit": bc}
            if f0 is None:
                counts["unread"] += 1
                print(f"UNREAD {citing}:{n} {spelled} ({kind}): written by merge {bc[:8]}, whose parents do not hold it")
                continue
            cited_b = show(b["commit"], path)
            cited_h = show(head, path)
            wb, cb = window(cited_b, f0, l0)
            wh, ch = window(cited_h, first, last_n)
            if wb is None or wh is None:
                counts["unread"] += 1
                print(f"UNREAD {citing}:{n} {spelled} ({kind}): out of range at {b['commit'][:8]} or the head")
                continue
            if wb == wh:
                counts["SAME"] += 1
                continue
            if not last_n and cb[0].startswith("|") and row_key(cb[0]) is not None and row_key(cb[0]) == row_key(ch[0]):
                counts["SAME-ROW"] += 1  # a table row rewritten in place: the same first cell (refs.py's rule)
                continue
            if cb == ch:
                counts["SAME-LINES"] += 1
                print(f"SAME-LINES {citing}:{n} {spelled} ({kind}; written by {b['commit'][:8]} as :{f0}): "
                      f"the cited text the same, a neighbour not")
                continue
            counts["WRONG"] += 1
            k = len(cb)
            at = [j + 1 for j in range(len(cited_h) - k + 1) if cited_h[j:j + k] == cb]
            if len(at) > 1:  # the cited text at several places: the one its two either side match too
                at = [j for j in at if window(cited_h, j, j + k - 1 if k > 1 else None)[0] == wb] or at
            where = f"its text is at :{at[0]}" + (f"-{at[0] + k - 1}" if k > 1 else "") if len(at) == 1 else f"its text is at {len(at)} places"
            if not at:  # rewritten: the head's line most like it, for a reader
                best = max(range(len(cited_h)), key=lambda j: difflib.SequenceMatcher(None, cb[0], cited_h[j], autojunk=False).quick_ratio())
                r = difflib.SequenceMatcher(None, cb[0], cited_h[best], autojunk=False).ratio()
                where += f"; the head's line most like its first is :{best + 1} (ratio {r:.2f})"
            print(f"WRONG {citing}:{n} {spelled} ({kind}; written by {b['commit'][:8]} as :{f0}"
                  f"{'-' + str(l0) if l0 else ''}): the head's line holds {ch[0].strip()[:50]!r}; "
                  f"{cb[0].strip()[:50]!r} -- {where}")
    print("# " + "; ".join(f"{k} {v}" for k, v in counts.items()) + f"; refs {sum(counts.values())}")
    sys.exit(1 if counts["WRONG"] else 0)


if __name__ == "__main__":
    main()
