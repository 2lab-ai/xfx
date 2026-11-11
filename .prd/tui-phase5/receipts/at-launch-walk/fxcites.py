"""Every fx v0.0.16 line this unit cites, checked against what `fx_src.sh`
quotes: each `file.zig:N` or `:N-M` in this folder's README, in the text
this unit put into `docs/parity.md` and in the source lines it added under
`src/` must fall inside one of fx_src.sh's `q <path> <first> <last>` ranges
for that file, or it is reported.

PAGE-KEYS-SETTINGS-COLUMN's checker, with two changes for AT-LAUNCH-WALK:
the source lines are a third input (each hunk's added lines, comment
markers stripped and joined, so a bare `:N` reads in the hunk's own file
context), and a citation wrapped after its colon (`foo.zig:` at a line's
end, the number on the next) is read whole, in all three inputs.

A bare `:N` belongs to the last `.zig` file named before it. A bare `:N`
with no `.zig` before it -- none at all, or a non-`.zig` file named in
between (an xfx `.rs`, a `.md`, a `.txt`) -- is a failure. A citation
pinned to another revision (`@c1db9191`) is not this pin's and is skipped
(and counted). In `docs/parity.md` only the citations this unit touched are
checked -- each one whose text overlaps a span this unit inserted, replaced
or deleted (difflib over each row, `<base>` against the working tree).

Exits 1 if any citation fails, else 0. `--self-test` checks the checker.

usage: python3 -I fxcites.py <base>   (run in the repo)
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


RECEIPT = ".prd/tui-phase5/receipts/at-launch-walk"
WRAP = re.compile(r"\.zig:\s*\n[ \t]*(?:///?|#)?[ \t]*(?=\d)")
MARK = re.compile(r"^\s*(?:///?!?|#)\s?")


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
    if spans is None:
        text = WRAP.sub(".zig:", text)
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


def source_hunks(base):
    """(label, text) for each hunk of `git diff -U0 <base> -- src`: its added
    lines, comment markers stripped, joined with newlines."""
    out = subprocess.run(["git", "diff", "-U0", base, "--", "src"], capture_output=True, text=True, check=True).stdout
    hunks, path, start, lines = [], None, 0, []
    def flush():
        if lines:
            hunks.append((f"{path}:{start}", "\n".join(lines)))
    for line in out.split("\n"):
        if line.startswith("+++ "):
            flush(); lines = []
            path = line[6:] if line.startswith("+++ b/") else line[4:]
        elif line.startswith("@@"):
            flush(); lines = []
            start = int(re.search(r"\+(\d+)", line)[1])
        elif line.startswith("+"):
            lines.append(MARK.sub("", line[1:]))
    flush()
    return [(label, context(label) + text) for label, text in hunks]


def context(label):
    """The `.zig` a hunk's first bare `:N` belongs to when the hunk starts
    inside a comment that named it above: the last one named in the comment
    lines just before the hunk (the file as it is now), as a bare name that
    sets the context and cites nothing."""
    path, start = label.rsplit(":", 1)
    lines = open(path, encoding="utf-8").read().split("\n")[: int(start) - 1]
    named = None
    for line in reversed(lines):
        if not line.strip().startswith("//"):
            break
        found = re.findall(r"([A-Za-z0-9_./-]+\.zig)", line)
        if found:
            named = found[-1]
            break
    return f"`{named}` " if named else ""


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
        ("a citation wrapped after its colon is read whole", "`foo.zig:\n/// 30`", None, 1),
        ("a wrapped quoted citation passes", "`foo.zig:\n  12-18`", None, 0),
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
    quoted = ranges(None)
    readme = read(os.path.join(HERE, "README.md"), None)
    t1, b1 = check("README.md", readme, quoted)
    old = subprocess.run(["git", "show", f"{base}:docs/parity.md"], capture_output=True, text=True, check=True).stdout.split("\n")
    new = read("docs/parity.md", None).split("\n")
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
    t3 = b3 = 0
    for label, text in source_hunks(base):
        t, b = check(label, text, quoted, say=lambda line: print(line) if "citations, 0 failed" not in line else None)
        t3 += t
        b3 += b
    print(f"src: {t3} citations, {b3} failed")
    print(f"all: {t1 + t2 + t3} citations, {b1 + b2 + b3} failed")
    return 1 if b1 + b2 + b3 else 0


if __name__ == "__main__":
    sys.exit(main())
