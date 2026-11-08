#!/usr/bin/env python3
"""The merge of main into STOP-LATE-EVENT: every xfx `file:line` reference in
the documents and the code, checked against the code it was written about.

    python3 -I -B refs.py <repo> <ours> <theirs> [--apply] [--doc <path>]...

A reference is `path:N` or `path:N-M` (a bare `:N` after one is the same
file) into an xfx file: `.rs`, `.sh`, `.py`, `.ts`, `.toml`, `.md`, `.yml`.
One pinned to a commit (`…@<sha>`) or into fx (`.zig`) is not checked. The
documents are `docs/parity.md`, `scripts/fxcap/parity-ledger.toml`,
`scripts/fxcap/capfile.py`, every `.rs` file under `src/`, and each `--doc`.
A bare `:N` is read only on the line its file is named on: one on the next
line (a wrapped comment) is the reader's to check.

A reference's **frame** is each parent whose copy of the document holds
that exact line: the code the reference was written about is that parent's
copy of the cited file. The reference **holds** when, for one of its
frames, the cited lines and two either side are the same text in the
working tree -- for a row of a `.md` table, the row alone, as rows are
long and a neighbour's edit does not move it. One that holds in no frame is **moved** when that window
occurs exactly once in the working tree (it is re-pointed there with
`--apply`), and **lost** otherwise (printed for a reader).

A reference into a file neither parent changed since the merge base cannot
move, and is counted, not read.
"""
import os
import re
import subprocess
import sys

EXT = r"(?:rs|sh|py|ts|toml|md|yml)"
REF = re.compile(r"(?<![\w/.@-])((?:[\w.-]+/)*[\w.-]+\." + EXT + r"):(\d+)(?:-(\d+))?(?![\w@])")
CONT = re.compile(r"(?<=[\s(`,;])`?:(\d+)(?:-(\d+))?(?![\w@])")
ANY = re.compile(r"(?<![\w/.@-])((?:[\w.-]+/)*[\w.-]+\.(?:zig|" + EXT[3:] + r"):\d+")
CONTEXT = 2


def git(repo, *args):
    return subprocess.run(["git", "-C", repo, *args], check=True, capture_output=True, text=True).stdout


def show(repo, commit, path, cache={}):
    key = (commit, path)
    if key not in cache:
        try:
            cache[key] = git(repo, "show", "%s:%s" % (commit, path)).split("\n")
        except subprocess.CalledProcessError:
            cache[key] = None
    return cache[key]


def main():
    repo, ours, theirs = sys.argv[1:4]
    apply = "--apply" in sys.argv
    extra = [sys.argv[i + 1] for i, a in enumerate(sys.argv) if a == "--doc"]
    base = git(repo, "merge-base", ours, theirs).strip()
    changed = set(git(repo, "diff", "--name-only", base, ours).split()) | set(
        git(repo, "diff", "--name-only", base, theirs).split())
    files = git(repo, "ls-files").split()
    by_name = {}
    for f in files:
        by_name.setdefault(os.path.basename(f), []).append(f)

    def resolve(text):
        if "/" in text:
            hits = [f for f in files if f == text or f.endswith("/" + text)]
        else:
            hits = by_name.get(text, [])
        return hits[0] if len(hits) == 1 else None

    docs = ["docs/parity.md", "scripts/fxcap/parity-ledger.toml", "scripts/fxcap/capfile.py"]
    docs += [f for f in files if f.startswith("src/") and f.endswith(".rs")]
    docs += extra
    counts = {"holds": 0, "moved": 0, "lost": 0, "unchanged file": 0, "ambiguous": 0}
    lost, moved = [], []
    for doc in docs:
        path = os.path.join(repo, doc)
        lines = open(path, encoding="utf-8").read().split("\n")
        frames = [(c, set(show(repo, c, doc) or [])) for c in (ours, theirs)]
        edits = []
        for i, line in enumerate(lines):
            last = None
            # Walk the line: file refs set `last`; a bare `:N` uses it.
            tokens = sorted(
                [(m.start(), "ref", m) for m in REF.finditer(line)]
                + [(m.start(), "cont", m) for m in CONT.finditer(line)]
                + [(m.start(), "any", m) for m in ANY.finditer(line)],
                key=lambda t: (t[0], t[1] != "ref"))
            for at, kind, m in tokens:
                if kind == "any":
                    if m.group(1).endswith(".zig"):
                        last = None
                    continue
                if kind == "ref":
                    target = resolve(m.group(1))
                    if target is None:
                        counts["ambiguous"] += 1
                        last = None
                        continue
                    last = target
                    a, b = int(m.group(2)), int(m.group(3) or m.group(2))
                    span = (m.start(2), m.end(3) if m.group(3) else m.end(2))
                else:
                    if last is None or any(t[0] < at < t[2].end() for t in tokens if t[1] == "ref"):
                        continue
                    target = last
                    a, b = int(m.group(1)), int(m.group(2) or m.group(1))
                    span = (m.start(1), m.end(2) if m.group(2) else m.end(1))
                if target not in changed:
                    counts["unchanged file"] += 1
                    continue
                now = open(os.path.join(repo, target), encoding="utf-8", errors="replace").read().split("\n")
                held, windows = False, []
                for commit, doc_lines in frames:
                    if line not in doc_lines:
                        continue
                    old = show(repo, commit, target)
                    if old is None or b > len(old):
                        continue
                    ctx = 0 if target.endswith(".md") else CONTEXT
                    lo, hi = max(a - 1 - ctx, 0), min(b + ctx, len(old))
                    win = old[lo:hi]
                    if now[lo:hi] == win:
                        held = True
                        break
                    windows.append((commit, lo, win, a - 1 - lo))
                if held:
                    counts["holds"] += 1
                    continue
                found = None
                for commit, lo, win, off in windows:
                    starts = [s for s in range(len(now) - len(win) + 1) if now[s:s + len(win)] == win]
                    if len(starts) == 1:
                        found = (commit, starts[0] + off + 1)
                        break
                where = "%s:%d %s:%s" % (doc, i + 1, target, "%d-%d" % (a, b) if b != a else a)
                if found and windows:
                    na = found[1]
                    nb = na + (b - a)
                    counts["moved"] += 1
                    moved.append("%s -> :%s (frame %s): %r" % (where, "%d-%d" % (na, nb) if nb != na else na,
                                                               found[0][:8], now[na - 1].strip()[:70]))
                    edits.append((i, span, "%d-%d" % (na, nb) if b != a else "%d" % na))
                elif windows:
                    counts["lost"] += 1
                    lost.append(where + " (in no frame's window found once)")
                else:
                    counts["lost"] += 1
                    lost.append(where + " (no frame holds this line)")
        if apply and edits:
            for i, (s, e), text in sorted(edits, key=lambda x: (x[0], -x[1][0])):
                lines[i] = lines[i][:s] + text + lines[i][e:]
            open(path, "w", encoding="utf-8").write("\n".join(lines))
    for line in moved:
        print("MOVED", line)
    for line in lost:
        print("LOST ", line)
    print("refs into changed files: %d hold, %d moved%s, %d lost; %d into unchanged files; %d names not resolved to one file"
          % (counts["holds"], counts["moved"], " (re-pointed)" if apply else "", counts["lost"],
             counts["unchanged file"], counts["ambiguous"]))
    sys.exit(1 if counts["lost"] or (counts["moved"] and not apply) else 0)


main()
