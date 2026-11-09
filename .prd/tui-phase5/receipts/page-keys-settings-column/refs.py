"""The source refs of the files this unit changed, as `docs/parity.md` and
`scripts/fxcap/parity-ledger.toml` name them, mapped from <base> to <head>.

`--rewrite` re-points each ref whose line moved (through `git diff -U0`'s
hunks: a line no hunk touched keeps its text and moves by the lines added
and removed above it) and writes both files; without it, it checks them as they are in the
tree (the head's, once committed): every ref must name a line of <head>
whose text is the line the same ref named at <base>, and a ref to a line this unit changed is reported, not
mapped.

usage: python3 -I refs.py <base> <head> [--rewrite]   (run in the repo)
"""

import re
import subprocess
import sys

sys.dont_write_bytecode = True
FILES = ["src/tui/shell.rs", "src/tui/settings.rs", "src/tui/model_column.rs"]
DOCS = ["docs/parity.md", "scripts/fxcap/parity-ledger.toml"]
REF = re.compile(r"(src/tui/(?:shell|settings|model_column)\.rs):(\d+)")


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, text=True, check=True).stdout


def mapping(base, head, path):
    """old line -> new line for every line no hunk touched, None for a
    line a hunk removed or replaced."""
    hunks = []
    for m in re.finditer(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", git("diff", "-U0", base, head, "--", path), re.M):
        o, oc, n, nc = int(m[1]), int(m[2] or 1), int(m[3]), int(m[4] or 1)
        hunks.append((o, oc, n, nc))

    def new_of(old):
        shift = 0
        for o, oc, n, nc in hunks:
            if oc == 0:  # `-o,0`: lines inserted after old line o
                if old > o:
                    shift += nc
            elif o <= old < o + oc:
                return None
            elif old >= o + oc:
                shift += nc - oc
        return old + shift

    return new_of


def main():
    base, head = sys.argv[1], sys.argv[2]
    rewrite = "--rewrite" in sys.argv[3:]
    maps = {path: mapping(base, head, path) for path in FILES}
    old_text = {path: git("show", f"{base}:{path}").split("\n") for path in FILES}
    new_text = {path: git("show", f"{head}:{path}").split("\n") for path in FILES}
    total = wrong = 0
    for doc in DOCS:
        before = git("show", f"{base}:{doc}")
        refs_before = REF.findall(before)
        if rewrite:
            def repl(m):
                new = maps[m[1]](int(m[2]))
                return f"{m[1]}:{new}" if new is not None else m[0]
            after = REF.sub(repl, open(doc, encoding="utf-8").read())
            open(doc, "w", encoding="utf-8").write(after)
            continue
        refs_after = REF.findall(open(doc, encoding="utf-8").read())
        for (path, o), (path2, n) in zip(refs_before, refs_after):
            o, n = int(o), int(n)
            total += 1
            mapped = maps[path](o)
            if mapped is None:
                print(f"{doc}: {path}:{o} -> :{n}  a line this unit changed; reread: {new_text[path][n - 1].strip()[:60]!r}")
                continue
            ok = path == path2 and mapped == n and old_text[path][o - 1] == new_text[path][n - 1]
            wrong += not ok
            print(f"{doc}: {path}:{o} -> :{n}  {'same text' if ok else 'WRONG'}  {new_text[path][n - 1].strip()[:60]!r}")
        if len(refs_before) != len(refs_after):
            print(f"{doc}: {len(refs_before)} refs at the base, {len(refs_after)} at the head")
    if not rewrite:
        print(f"refs: {total}; wrong: {wrong}")


if __name__ == "__main__":
    main()
