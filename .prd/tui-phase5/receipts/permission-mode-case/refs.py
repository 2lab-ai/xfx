"""Every `path:N` (or `path:A-B`) reference in the tracked tree that points
into a file this unit changed, checked by text: the line it names at the head
must be the line it named at the base. A reference whose text moved is
re-pointed with `--apply` to where that same line now stands (difflib's map of
the unchanged lines); one whose line was itself changed is listed for a hand.
Receipts and the historical plans (`docs/superpowers/plans/`) are history,
each citing its own head, and are left alone, as are references this unit
added (no base counterpart). A bare `<basename>:N` naming a changed file is
reported, not re-pointed, when the basename is not unique in the tree.

usage: python3 -I refs.py <base> <head | WORKTREE> [--apply]   (run in the repo)
"""

import difflib
import re
import subprocess
import sys

sys.dont_write_bytecode = True
REF = re.compile(r"(?<![\w/.-])((?:src|tests|scripts|docs)/[\w./-]+?\.(?:rs|py|sh|toml|md|cap|ts|txt)):(\d+)(?:-(\d+))?(?![\w@])")


def git(*args):
    out = subprocess.run(["git", *args], capture_output=True, check=True).stdout
    return out.decode("utf-8")


def read(rev, path):
    if rev == "WORKTREE":
        try:
            return open(path, encoding="utf-8").read()
        except (FileNotFoundError, UnicodeDecodeError):
            return None
    try:
        return git("show", f"{rev}:{path}")
    except (subprocess.CalledProcessError, UnicodeDecodeError):
        return None


def line_map(old, new):
    sm = difflib.SequenceMatcher(None, old, new, autojunk=False)
    m = {}
    for a, b, size in sm.get_matching_blocks():
        for i in range(size):
            m[a + i + 1] = b + i + 1
    return m


def main():
    base, head = sys.argv[1], sys.argv[2]
    apply = "--apply" in sys.argv
    if head == "WORKTREE":
        changed = git("diff", "--name-only", base).split()
    else:
        changed = git("diff", "--name-only", base, head).split()
    changed = set(changed)
    targets = {}
    for path in changed:
        old, new = read(base, path), read(head, path)
        if old is not None and new is not None:
            targets[path] = (old.split("\n"), new.split("\n"))
    maps = {p: line_map(o, n) for p, (o, n) in targets.items()}
    files = [
        f
        for f in git("ls-files").split("\n")
        if f and "/receipts/" not in f and not f.startswith("docs/superpowers/plans/")
    ]
    moved = same = by_hand = 0
    for f in files:
        before, after = read(base, f), read(head, f)
        if before is None or after is None:
            continue
        base_refs = {m.group(0) for m in REF.finditer(before) if m.group(1) in targets}
        edits = {}
        for m in REF.finditer(after):
            path = m.group(1)
            if path not in targets or m.group(0) not in base_refs:
                continue
            old, new = targets[path]
            nums = [int(m.group(2))] + ([int(m.group(3))] if m.group(3) else [])
            if all(0 < n <= len(old) and n <= len(new) and old[n - 1] == new[n - 1] for n in nums):
                same += 1
                continue
            mapped = [maps[path].get(n) for n in nums]
            if None in mapped:
                by_hand += 1
                print(f"BY HAND {f}: {m.group(0)} -- its line changed: {old[nums[0] - 1].strip()[:70]!r}")
                continue
            ref = f"{path}:{mapped[0]}" + (f"-{mapped[1]}" if len(mapped) > 1 else "")
            edits[m.group(0)] = ref
            moved += 1
            print(f"MOVED {f}: {m.group(0)} -> {ref}  {new[mapped[0] - 1].strip()[:70]!r}")
        if apply and edits and head == "WORKTREE":
            text = after
            for old_ref, new_ref in edits.items():
                text = re.sub(re.escape(old_ref) + r"(?![\d@-])", new_ref, text)
            open(f, "w", encoding="utf-8").write(text)
    print(f"== refs into changed files: {same} same text, {moved} moved, {by_hand} by hand")
    bare(base, head, files, targets)


BARE = re.compile(r"(?<![\w/.-])([\w-]+\.(?:rs|md|toml|cap|py|sh)):(\d+)(?:-(\d+))?(?![\w@])")


def bare(base, head, files, targets):
    """Bare `<basename>:N` references whose basename is a changed file's: the
    ones whose number names a line that moved there, for a hand to judge
    (a basename that is not unique may mean another file)."""
    tracked = [f for f in git("ls-files").split("\n") if f]
    count = {}
    for f in tracked:
        name = f.rsplit("/", 1)[-1]
        count[name] = count.get(name, 0) + 1
    by_name = {}
    for path in targets:
        by_name.setdefault(path.rsplit("/", 1)[-1], []).append(path)
    flagged = 0
    for f in files:
        before, after = read(base, f), read(head, f)
        if before is None or after is None:
            continue
        base_refs = {m.group(0) for m in BARE.finditer(before)}
        for m in BARE.finditer(after):
            paths = by_name.get(m.group(1))
            if not paths or m.group(0) not in base_refs:
                continue
            for path in paths:
                old, new = targets[path]
                nums = [int(m.group(2))] + ([int(m.group(3))] if m.group(3) else [])
                if all(0 < n <= len(old) and n <= len(new) and old[n - 1] == new[n - 1] for n in nums):
                    continue
                flagged += 1
                print(f"BARE {f}: {m.group(0)} ({count.get(m.group(1), 0)} tracked files so named; if {path}, its line moved)")
    print(f"== bare refs to judge by hand: {flagged}")


if __name__ == "__main__":
    main()
