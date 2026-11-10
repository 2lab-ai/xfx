"""Every fx v0.0.16 line this unit cites, checked against what `fx_src.sh`
quotes: each `file.zig:N` or `:N-M` in this folder's README and in the text
this unit put into `docs/parity.md` must fall inside one of fx_src.sh's
`q <path> <first> <last>` ranges for that file, or it is reported.

A bare `:N` belongs to the last `.zig` file named before it; a non-`.zig`
file named in between (an xfx `.rs`, a `.md`) ends that. A citation pinned
to another revision (`@c1db9191`) is not this pin's and is skipped. In
`docs/parity.md` only the spans this unit inserted are checked (difflib over
each row, `<base>` against the working tree), with the file context of the
whole row.

With `--at <rev>` the three files are read as they were at that commit
(round 2's head, to show what this check found there).

usage: python3 -I fxcites.py <base> [--at <rev>]   (run in the repo)
"""

import difflib
import os
import re
import subprocess
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.realpath(__file__))
TOKEN = re.compile(r"([A-Za-z0-9_./-]+\.(?:zig|rs|md|toml|py|sh|txt|json|cap))(?::(\d+)(?:-(\d+))?)?(@[0-9a-f]+)?|(?<![\w.])(:(\d+)(?:-(\d+))?)(@[0-9a-f]+)?")


RECEIPT = ".prd/tui-phase5/receipts/page-keys-settings-column"


def read(path, at):
    if at is None:
        return open(path, encoding="utf-8").read()
    return subprocess.run(["git", "show", f"{at}:{path}"], capture_output=True, text=True, check=True).stdout


def ranges(at):
    out = {}
    for line in read(os.path.join(HERE, "fx_src.sh") if at is None else f"{RECEIPT}/fx_src.sh", at).split("\n"):
        m = re.match(r"q (\S+) (\d+) (\d+)", line)
        if m:
            out.setdefault(m[1], []).append((int(m[2]), int(m[3])))
    return out


def resolve(name, quoted):
    hits = [path for path in quoted if path == name or path.endswith("/" + name)]
    return hits[0] if len(hits) == 1 else (name if not hits else None)


def cites(text, spans=None):
    """(position, file, first, last) for every fx citation in `text`; with
    `spans`, only those starting inside one."""
    current = None
    for m in TOKEN.finditer(text):
        if m[1]:
            if not m[1].endswith(".zig"):
                current = None
                continue
            current = m[1]
            if m[2] is None:
                continue
            first, last, pin = int(m[2]), int(m[3] or m[2]), m[4]
        else:
            if current is None:
                continue
            first, last, pin = int(m[6]), int(m[7] or m[6]), m[8]
        if pin and pin != "@481104fd":
            continue
        if spans is not None and not any(a <= m.start() < b for a, b in spans):
            continue
        yield m.start(), current, first, last


def check(label, text, quoted, spans=None):
    bad = total = 0
    for _, name, first, last in cites(text, spans):
        total += 1
        path = resolve(name, quoted)
        ok = path in quoted and any(a <= first and last <= b for a, b in quoted[path])
        bad += not ok
        if not ok:
            print(f"{label}: {name}:{first}-{last}  NOT QUOTED")
    print(f"{label}: {total} citations, {bad} not quoted")
    return total, bad


def main():
    base = sys.argv[1]
    at = sys.argv[sys.argv.index("--at") + 1] if "--at" in sys.argv else None
    quoted = ranges(at)
    readme = read(os.path.join(HERE, "README.md") if at is None else f"{RECEIPT}/README.md", at)
    t1, b1 = check("README.md", readme, quoted)
    old = subprocess.run(["git", "show", f"{base}:docs/parity.md"], capture_output=True, text=True, check=True).stdout.split("\n")
    new = read("docs/parity.md", at).split("\n")
    t2 = b2 = 0
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, old, new, autojunk=False).get_opcodes():
        if tag == "equal":
            continue
        for j in range(j1, j2):
            row = new[j]
            before = old[i1 + (j - j1)] if tag == "replace" and i1 + (j - j1) < i2 else ""
            spans = [(b1_, b2_) for t, a1, a2, b1_, b2_ in difflib.SequenceMatcher(None, before, row, autojunk=False).get_opcodes() if t in ("insert", "replace")]
            t, b = check(f"docs/parity.md:{j + 1}", row, quoted, spans)
            t2 += t
            b2 += b
    print(f"all: {t1 + t2} citations, {b1 + b2} not quoted")


if __name__ == "__main__":
    main()
