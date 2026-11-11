"""Every fx v0.0.16 line this unit cites, checked against what `fx_src.sh`
quotes: each `file.zig:N` or `:N-M` in this folder's README and in the text
this unit put into `docs/parity.md` must fall inside one of fx_src.sh's
`q <path> <first> <last>` ranges for that file, or it is reported.

A bare `:N` belongs to the last `.zig` file named before it. A bare `:N`
with no `.zig` before it -- none at all, or a non-`.zig` file named in
between (an xfx `.rs`, a `.md`, a `.txt`) -- is a failure: it is either an
fx line whose file is not said, or another file's line written so it reads
like one. A citation pinned to another revision (`@c1db9191`) is not this
pin's and is skipped (and counted). In `docs/parity.md` only the citations
this unit touched are checked -- each one whose text overlaps a span this
unit inserted, replaced or deleted (difflib over each row, `<base>` against
the working tree), so a number changed inside an older citation is checked
too -- with the file context of the whole row.

Exits 1 if any citation fails, else 0. `--self-test` checks the checker: a
quoted citation passes, and an unquoted one, a bare `:N` with no `.zig`
before it, a bare `:N` after an `.rs` and an older citation whose number was
changed each fail; it exits 1 unless every case comes out as planted.

With `--at <rev>` the three files are read as they were at that commit
(round 2's head, to show what this check found there).

usage: python3 -I fxcites.py <base> [--at <rev>]   (run in the repo)
       python3 -I fxcites.py --self-test
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
    """(position, file, first, last, pin) for every fx citation in `text`,
    the file None for a bare `:N` with no `.zig` before it; with `spans`,
    only those whose text overlaps one (a deletion's span is empty)."""
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
            first, last, pin = int(m[6]), int(m[7] or m[6]), m[8]
        if spans is not None and not any(m.start() <= b and a <= m.end() for a, b in spans):
            continue
        yield m.start(), current, first, last, pin


def check(label, text, quoted, spans=None, say=print):
    bad = total = other = 0
    for _, name, first, last, pin in cites(text, spans):
        if name is None:
            total += 1
            bad += 1
            say(f"{label}: :{first}-{last}  BARE (no .zig before it)")
            continue
        if pin and pin != "@481104fd":
            other += 1
            continue
        total += 1
        path = resolve(name, quoted)
        ok = path in quoted and any(a <= first and last <= b for a, b in quoted[path])
        bad += not ok
        if not ok:
            say(f"{label}: {name}:{first}-{last}  NOT QUOTED")
    say(f"{label}: {total} citations, {bad} failed" + (f", {other} of another pin skipped" if other else ""))
    return total, bad


def touched(before, row):
    """The spans of `row` this edit inserted, replaced or deleted."""
    ops = difflib.SequenceMatcher(None, before, row, autojunk=False).get_opcodes()
    return [(b1, b2) for t, _, _, b1, b2 in ops if t != "equal"]


def self_test():
    """Plants each kind of citation and says whether the check comes out as
    planted; exits 1 unless all do."""
    quoted = {"src/a/foo.zig": [(10, 20)]}
    cases = [
        ("a quoted citation passes", "see `foo.zig:12-18`", None, 0),
        ("a quoted bare :N after its .zig passes", "`foo.zig:10`, `:15`", None, 0),
        ("an unquoted citation fails", "see `foo.zig:30`", None, 1),
        ("a bare :N with no .zig before it fails", "the list first (`:12`)", None, 1),
        ("a bare :N after an .rs fails", "`foo.zig:12` and `src/x.rs:5`, `:15`", None, 1),
        ("another pin's citation is skipped", "`foo.zig:99@c1db9191`", None, 0),
    ]
    before, after = "an old row cites `foo.zig:12-18` here", "an old row cites `foo.zig:12-28` here"
    cases.append(("a number changed inside an older citation fails", after, touched(before, after), 1))
    cases.append(("an older citation left alone is not this unit's", after, touched(after, after + " more"), 0))
    wrong = 0
    for name, text, spans, want in cases:
        _, bad = check("self-test", text, quoted, spans, say=lambda _line: None)
        ok = bad == want
        wrong += not ok
        print(f"{'ok  ' if ok else 'WRONG'} {name}: {bad} failed, planted {want}")
    print(f"self-test: {len(cases)} cases, {wrong} wrong")
    return 1 if wrong else 0


def main():
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
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
            t, b = check(f"docs/parity.md:{j + 1}", row, quoted, touched(before, row))
            t2 += t
            b2 += b
    print(f"all: {t1 + t2} citations, {b1 + b2} failed")
    return 1 if b1 + b2 else 0


if __name__ == "__main__":
    sys.exit(main())
