#!/usr/bin/env python3
"""Train 13: the sweep's selection, derived. The brief's globs, then every
scenario the ledger entries the merges touched name: each `[[...]]` entry of
scripts/fxcap/parity-ledger.toml whose text differs between a merge commit's
first parent and the merge (added, removed or changed), keyed by its `id`,
and the scenarios its `scenarios = [...]` list names (a removed entry's from
the parent, the rest from the merge) -- a name with a `*` is a glob and goes
in as one. A name that is no scenario at the head (and no glob) is listed
and left out. The entries the re-point commit changed are listed too, not
selected: it changed only the numbers of `xfx` and `source` citations, which
the sweep does not score with (ledger.py reads them for the file and the
range alone, `_sources`).

    python3 -I -B selection.py <repo> <repoint> <merge>... > selection.txt

The output is a file of `--only` globs, one per line, `#` lines comments:
sweep.sh's SELECTION.
"""
import re
import subprocess
import sys

BRIEF = ["approvals*", "p5-approval*", "question*", "p5-question*", "p5-input-question*", "composer-keys",
         "question-field", "resume*", "undo-resume", "image*", "p5-lifecycle*", "provider-carry-back*"]
L = "scripts/fxcap/parity-ledger.toml"


def git(repo, *a):
    return subprocess.run(["git", "-C", repo, *a], check=True, capture_output=True, text=True).stdout


def entries(text):
    out = {}
    starts = [m.start() for m in re.finditer(r"^\[\[", text, re.M)] + [len(text)]
    for a, b in zip(starts, starts[1:]):
        body = text[a:b]
        kind = re.match(r"\[\[(\w+)\]\]", body).group(1)
        m = re.search(r'^id = "([^"]+)"', body, re.M)
        key = (kind, m.group(1) if m else body.split("\n", 1)[0])
        out[key] = body.rstrip("\n")
    return out


def scenarios(body):
    m = re.search(r"^scenarios = \[([^\]]*)\]", body, re.M)
    return re.findall(r'"([^"]+)"', m.group(1)) if m else []


repo, repoint, merges = sys.argv[1], sys.argv[2], sys.argv[3:]
have = {p.rsplit("/", 1)[-1][:-4] for p in git(repo, "ls-tree", "--name-only", f"{repoint}:scripts/fxcap/scenarios").split()
        if p.endswith(".cap")}
print(f"# train 13's sweep selection: the brief's globs, then the scenarios named by the ledger entries "
      f"the merges ({', '.join(m[:8] for m in merges)}) touched")
for g in BRIEF:
    print(g)
named = {}
for m in merges + [repoint]:
    old, new = entries(git(repo, "show", f"{m}^1:{L}")), entries(git(repo, "show", f"{m}:{L}"))
    changed = sorted(k for k in set(old) | set(new) if old.get(k) != new.get(k))
    print(f"# {m[:8]} ({'the re-point: listed, not selected' if m == repoint else 'a merge'}): {len(changed)} entries")
    for k in changed:
        body = new.get(k) or old.get(k)
        state = "removed" if k not in new else "added" if k not in old else "changed"
        print(f"#   {k[0]} {k[1]} ({state}): {', '.join(scenarios(body)) or 'names no scenario'}")
        if m != repoint:
            for sc in scenarios(body):
                named.setdefault(sc, []).append(k[1])
import fnmatch  # noqa: E402
for sc in sorted(named):
    if any(fnmatch.fnmatch(sc, g) for g in BRIEF):
        print(f"# {sc}: in the brief's globs already")
    elif "*" in sc:
        hits = sorted(h for h in have if fnmatch.fnmatch(h, sc))
        print(f"# {sc}: a glob, {len(hits)} scenarios at the head")
        print(sc)
    elif sc not in have:
        print(f"# {sc}: no scenario at the head, left out")
    else:
        print(sc)
