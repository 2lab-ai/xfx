#!/usr/bin/env python3
# Train 13: train 12's refs_all.py, unchanged but for this line.
"""Every unpinned `path:line` reference in the tracked files outside `.prd/`
(src, tests, scripts, docs, and the rest), mapped from each side's frame to
the train head -- train 3's refs.py widened from the ledger and
docs/parity.md to every citing file.

    python3 -I -B refs_all.py <repo> <head> <side>=<commit>...

A reference is a (citing file, cited path, line or range) the head's copy of
the citing file holds. For each side whose own copy of that citing file holds
the same reference, the cited line is mapped from the side's copy of the
cited file to the head's (`git diff -U0`); one that maps elsewhere is MOVED,
one inside a changed hunk is CHANGED. `docs/parity.md:<line>` counts as the
same when the line holds a table row with the same first cell on both. Only
the MOVED and CHANGED are printed, with a count of all.
"""
import functools
import re
import subprocess
import sys

REF = re.compile(r"\b((?:src|scripts|tests|docs)/[A-Za-z0-9_./-]+\.(?:rs|sh|py|md|toml|ts|json|yml)):(\d+)(?:-(\d+))?(?!\d|-\d|@)")


def git(repo, *args):
    return subprocess.run(["git", "-C", repo, *args], check=True, capture_output=True, text=True).stdout


@functools.lru_cache(maxsize=None)
def show(repo, commit, path):
    try:
        return git(repo, "show", f"{commit}:{path}")
    except subprocess.CalledProcessError:
        return None


@functools.lru_cache(maxsize=None)
def hunks(repo, old, new, path):
    found = []
    for line in git(repo, "diff", "-U0", old, new, "--", path).splitlines():
        m = re.match(r"@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", line)
        if m:
            a, b, c, d = m.groups()
            found.append((int(a), int(b) if b is not None else 1, int(c), int(d) if d is not None else 1))
    return tuple(found)


def mapped(line, spans):
    offset = 0
    for old_start, old_len, _new_start, new_len in spans:
        if old_len == 0:
            if line > old_start:
                offset += new_len
            continue
        if old_start <= line < old_start + old_len:
            return None
        if line >= old_start + old_len:
            offset += new_len - old_len
    return line + offset


def row_key(text, line):
    rows = (text or "").splitlines()
    if len(rows) >= line and rows[line - 1].startswith("|"):
        return rows[line - 1].split("|")[1].strip()
    return None


def main():
    repo, head, sides = sys.argv[1], sys.argv[2], [arg.split("=", 1) for arg in sys.argv[3:]]
    files = [f for f in git(repo, "ls-tree", "-r", "--name-only", head).splitlines()
             if not f.startswith(".prd/") and re.search(r"\.(rs|sh|py|md|toml|ts|json|yml)$", f)]
    total = flagged = 0
    print(f"# head {head}; sides: " + ", ".join(f"{n} {c}" for n, c in sides))
    for citing in files:
        text = show(repo, head, citing) or ""
        for m in sorted(set((m.group(0), m.group(1), int(m.group(2)), m.group(3)) for m in REF.finditer(text))):
            spelled, path, first, last = m
            if show(repo, head, path) is None:
                continue
            total += 1
            cells = []
            for name, commit in sides:
                if spelled not in (show(repo, commit, citing) or "") or show(repo, commit, path) is None:
                    continue
                if path == "docs/parity.md" and not last:
                    k = row_key(show(repo, commit, path), first)
                    if k is not None and k == row_key(show(repo, head, path), first):
                        continue
                spans = hunks(repo, commit, head, path)
                ends = [mapped(first, spans)] + ([mapped(int(last), spans)] if last else [])
                if any(e is None for e in ends):
                    cells.append(f"{name} CHANGED")
                elif ends[0] != first or (last and ends[-1] != int(last)):
                    cells.append(f"{name} MOVED -> " + "-".join(str(e) for e in ends))
            if cells:
                flagged += 1
                print(f"{citing}: {spelled}\n    " + " | ".join(cells))
    print(f"# {total} references in {len(files)} files; {flagged} moved or changed on some side that cites them")


if __name__ == "__main__":
    main()
