#!/usr/bin/env python3
# Train 14: train 13's selection.py with this train's brief's globs, the
# ledger commits after the merges read as the merges are (8f622949 deletes a
# cause), and one rule added: every scenario a merge adds (a `.cap` file the
# merge has and its first parent has not), so each unit's own scenarios are
# swept whatever the globs say. `--extra` is train 13's, unchanged.
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
    python3 -I -B selection.py --extra <repo> <repoint> <merge>... > selection-extra.txt

`--extra` (added after the first sweeps, to close the reading's gap): the
scenarios the re-point's entries name -- the entries whose cited lines the
merges moved, merge 3's twelve conflict blocks among them -- that the first
selection did not take, a `*` left out (every scenario: `session-record`).

The output is a file of `--only` globs, one per line, `#` lines comments:
sweep.sh's SELECTION.
"""
import re
import subprocess
import sys

BRIEF = ["filepicker*", "p5-input-at*", "p5-input-row-limit*", "workspace*", "p5-ctrlo*",
         "p5-page-keys*", "settings*", "image*", "p5-image*",
         "model-column*", "p5-panels-help", "fulltranscript*"]
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


extra = sys.argv[1] == "--extra"
args = sys.argv[2:] if extra else sys.argv[1:]
first_file = args.pop() if args[-1].endswith(".txt") else None  # --extra: the first selection
repo, repoint, merges = args[0], args[1], args[2:]
have = {p.rsplit("/", 1)[-1][:-4] for p in git(repo, "ls-tree", "--name-only", f"{repoint}:scripts/fxcap/scenarios").split()
        if p.endswith(".cap")}
named, later = {}, {}
if extra:
    import fnmatch  # noqa: E402
    old, new = entries(git(repo, "show", f"{repoint}^1:{L}")), entries(git(repo, "show", f"{repoint}:{L}"))
    for k in sorted(kk for kk in set(old) | set(new) if old.get(kk) != new.get(kk)):
        for sc in scenarios(new.get(k) or old.get(k)):
            later.setdefault(sc, []).append(k[1])
    print(f"# train 14's extra selection: the scenarios the {repoint[:8]} re-point's entries name that the first "
          f"selection ({first_file.rsplit('/', 1)[-1] if first_file else 'the brief globs'}) did not take")
    first = BRIEF + ([ln.strip() for ln in open(first_file) if ln.strip() and not ln.startswith("#")]
                     if first_file else [])
    have = {p.rsplit("/", 1)[-1][:-4] for p in git(repo, "ls-tree", "--name-only", f"{repoint}:scripts/fxcap/scenarios").split()
            if p.endswith(".cap")}
    for sc in sorted(later):
        why = ", ".join(later[sc])
        if sc == "*":
            print(f"# *: every scenario ({why}): left out")
        elif any(fnmatch.fnmatch(sc, g) for g in first):
            print(f"# {sc} ({why}): in the first selection")
        elif "*" in sc and not any(fnmatch.fnmatch(h, sc) for h in have):
            print(f"# {sc} ({why}): a glob no scenario at the head matches")
        elif "*" not in sc and sc not in have:
            print(f"# {sc} ({why}): no scenario at the head")
        else:
            print(f"# {sc}: {why}")
            print(sc)
    sys.exit(0)
print(f"# train 14's sweep selection: the brief's globs, then the scenarios named by the ledger entries "
      f"the merges and the ledger commits after them ({', '.join(m[:8] for m in merges)}) touched, then the "
      f"scenarios the merges added")
for g in BRIEF:
    print(g)
for m in merges + [repoint]:
    old, new = entries(git(repo, "show", f"{m}^1:{L}")), entries(git(repo, "show", f"{m}:{L}"))
    changed = sorted(k for k in set(old) | set(new) if old.get(k) != new.get(k))
    what = "the re-point: listed, not selected" if m == repoint else \
        "a merge" if len(git(repo, "rev-list", "--parents", "-n1", m).split()) > 2 else "a ledger commit"
    print(f"# {m[:8]} ({what}): {len(changed)} entries")
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
for m in merges:
    if len(git(repo, "rev-list", "--parents", "-n1", m).split()) < 3:
        continue
    added = [p for p in git(repo, "diff", "--name-only", "--diff-filter=A", f"{m}^1", m, "--",
                            "scripts/fxcap/scenarios/").split() if p.endswith(".cap")]
    for p in sorted(added):
        sc = p.rsplit("/", 1)[-1][:-4]
        if any(fnmatch.fnmatch(sc, g) for g in BRIEF) or sc in named:
            print(f"# {sc}: added by {m[:8]}, selected already")
        else:
            print(f"# {sc}: added by {m[:8]}")
            print(sc)
