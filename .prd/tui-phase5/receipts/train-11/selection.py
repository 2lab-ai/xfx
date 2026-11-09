#!/usr/bin/env python3
"""Train 11's sweep selection (train 10's selection.py, its globs this
train's): the brief's globs, each unit's own sweep's `--only` globs (so the
compare with each unit covers all its pairs), and the scenarios of every
ledger entry the train changed (an entry added, deleted, or with any field
other at <head> than at <base>; numbers included, so the conflict blocks'
entries and every re-pointed one are in). An entry naming every scenario
(`*`) adds none. Each glob kept only if it matches a scenario at <head>.

    python3 -I -B selection.py <repo> <base> <head> > selection.txt
"""
import fnmatch
import subprocess
import sys
import tomllib

repo, base, head = sys.argv[1:4]
BRIEF = [
    ("the submit scenarios", ["p5-submit-preview-*", "*submit*"]),
    ("the model and catalog scenarios", ["model*", "p5-input-model*", "p5-catalog-*"]),
    ("the approval and paste scenarios", ["approvals*", "paste*", "p5-paste*", "p5-input-paste*"]),
    # So that each unit's own sweep is compared over all its pairs: each
    # one's `--only` list, as its scoreboard prints it.
    ("SUBMIT-CARD-PREVIEW's own sweep's", ["p5-transcript-*", "p5-submit-preview-*"]),
    ("CATALOG-REFRESH's own sweep's", ["p5-input-model*", "model*", "h1-models*", "settings-model*"]),
    ("PASTE-GAP's own sweep's", [
        "approvals-amend-limit", "approvals-big", "approvals", "card-wrap", "compact-toolarge", "composer-wrap",
        "image-composer", "image-inline", "image-undo", "model-effort", "model-router", "model-speed",
        "p5-effort-fork-effort-column", "p5-effort-fork-effort-menu", "p5-effort-fork-effort-nolevels",
        "p5-effort-fork-fork-edge", "p5-input-approval", "p5-input-at-dir", "p5-input-at", "p5-input-ctrlbs",
        "p5-input-mcp", "p5-input-model-fail", "p5-input-model", "p5-input-paste-amend", "p5-input-paste-behind",
        "p5-input-paste-field", "p5-input-paste", "p5-input-question", "p5-input-smoke", "p5-panels-keys",
        "p5-session-early-prefs", "p5-transcript-submit-multi", "p5-transcript-submit", "p5-transcript-tallfirst",
        "paste-block-undo", "paste-card", "paste-suffix", "question-field", "question-other", "question",
        "skills-paste"]),
]


def git(*a):
    return subprocess.run(["git", "-C", repo, *a], check=True, capture_output=True, text=True).stdout


def entries(commit):
    doc = tomllib.loads(git("show", f"{commit}:scripts/fxcap/parity-ledger.toml"))
    out = {}
    for table, items in doc.items():
        if isinstance(items, list):
            for item in items:
                if isinstance(item, dict) and "id" in item:
                    out[(table, item["id"])] = item
    return out


names = sorted(f[len("scripts/fxcap/scenarios/"):-len(".cap")]
               for f in git("ls-tree", "-r", "--name-only", head, "scripts/fxcap/scenarios/").split()
               if f.endswith(".cap") and f.count("/") == 3)
eb, eh = entries(base), entries(head)
touched = sorted(k for k in set(eb) | set(eh) if eb.get(k) != eh.get(k))
print(f"# Train 11's sweep selection (selection.py {base[:8]} <head>); {len(names)} scenarios at the head.")
seen = []
for why, globs in BRIEF:
    print(f"# {why} (the brief's)")
    for g in globs:
        hit = [n for n in names if fnmatch.fnmatch(n, g)]
        if hit and g not in seen:
            seen.append(g)
            print(g)
print(f"# the scenarios of the {len(touched)} ledger entries the train changed")
for table, ident in touched:
    item = eh.get((table, ident)) or eb.get((table, ident))
    globs = [g for g in item.get("scenarios", []) if g != "*"]
    state = "added" if (table, ident) not in eb else "deleted" if (table, ident) not in eh else "changed"
    print(f"#   {table} {ident} ({state}): {', '.join(item.get('scenarios', [])) or '-'}")
    for g in globs:
        if g not in seen and any(fnmatch.fnmatch(n, g) for n in names):
            seen.append(g)
            print(g)
chosen = sorted({n for n in names if any(fnmatch.fnmatch(n, g) for g in seen)})
# A scenario written once per side (`<name>-fx.cap`, `<name>-xfx.cap`) is one
# scenario to the sweep.
folded = sorted({n[: -len("-xfx")] if n.endswith("-xfx") and n[: -len("-xfx")] + "-fx" in names
                 else n[: -len("-fx")] if n.endswith("-fx") and n[: -len("-fx")] + "-xfx" in names
                 else n for n in chosen})
print(f"# {len(folded)} scenarios ({len(chosen)} .cap files; a scenario with one per side counts once)")
