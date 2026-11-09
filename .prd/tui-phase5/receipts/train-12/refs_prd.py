#!/usr/bin/env python3
"""Train 12: train 7's refs.py with its pattern moved to the `.prd/` files
the ledger, docs/parity.md and capfile.py cite (a receipt's README, a
loop file) -- which refs.py's `src|scripts|tests|docs` pattern does not
read, and which LOCAL-PATHS rewrote (it wrote out the paths in 160 tracked
files, many of them receipts under `.prd/`). Everything else is refs.py's.

    python3 -I -B refs_prd.py <repo> <head> <side>=<commit>...
"""
import re
import subprocess
import sys

REF = re.compile(r"(?<![A-Za-z0-9_./-])(\.prd/[A-Za-z0-9_./-]+\.(?:md|txt|sh|py|toml|tsv|json)):(\d+)(?:-(\d+))?(?!\d|-\d|@)")
CITING = ["scripts/fxcap/parity-ledger.toml", "docs/parity.md", "scripts/fxcap/capfile.py"]


def git(repo, *args):
    return subprocess.run(["git", "-C", repo, *args], check=True, capture_output=True, text=True).stdout


def show(repo, commit, path):
    try:
        return git(repo, "show", f"{commit}:{path}")
    except subprocess.CalledProcessError:
        return None


def hunks(repo, old, new, path):
    out = git(repo, "diff", "-U0", old, new, "--", path)
    found = []
    for line in out.splitlines():
        m = re.match(r"@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", line)
        if m:
            a, b, c, d = m.groups()
            found.append((int(a), int(b) if b is not None else 1, int(c), int(d) if d is not None else 1))
    return found


def mapped(line, spans):
    offset = 0
    for old_start, old_len, new_start, new_len in spans:
        if old_len == 0:
            # A pure insertion after `old_start`.
            if line > old_start:
                offset += new_len
            continue
        if old_start <= line < old_start + old_len:
            return None
        if line >= old_start + old_len:
            offset += new_len - old_len
    return line + offset


def main():
    repo, head, sides = sys.argv[1], sys.argv[2], [arg.split("=", 1) for arg in sys.argv[3:]]
    refs = {}
    for citing in CITING:
        text = show(repo, head, citing)
        for number, row in enumerate(text.splitlines(), 1):
            for m in REF.finditer(row):
                refs.setdefault((m.group(1), int(m.group(2)), m.group(3)), []).append(f"{citing}:{number}")
    moved = 0
    print(f"# head {head}; sides: " + ", ".join(f"{name} {commit}" for name, commit in sides))
    for (path, first, last), where in sorted(refs.items()):
        spelled = f"{path}:{first}" + (f"-{last}" if last else "")
        cells = []
        for name, commit in sides:
            cites = any(spelled in (show(repo, commit, citing) or "") for citing in CITING)
            if path == "docs/parity.md" and not last:
                key = lambda text: (text.splitlines()[first - 1].split("|")[1].strip()
                                    if text and len(text.splitlines()) >= first
                                    and text.splitlines()[first - 1].startswith("|") else None)
                here, there = key(show(repo, commit, path)), key(show(repo, head, path))
                if here is not None and here == there:
                    cells.append(f"{name}{'*' if cites else ''} = row")
                    continue
            spans = hunks(repo, commit, head, path)
            ends = [mapped(first, spans)] + ([mapped(int(last), spans)] if last else [])
            if any(end is None for end in ends):
                state = "changed"
            elif ends[0] == first and (not last or ends[-1] == int(last)):
                state = "="
            else:
                state = "-> " + "-".join(str(end) for end in ends)
            flag = " MOVED" if cites and state != "=" else ""
            moved += bool(flag)
            cells.append(f"{name}{'*' if cites else ''} {state}{flag}")
        print(f"{spelled}\t[{', '.join(where)}]\n    " + " | ".join(cells))
    print(f"# {len(refs)} references; {moved} (side, reference) pairs MOVED; * = the side's own text cites it")


main()
