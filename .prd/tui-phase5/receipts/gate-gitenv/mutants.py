#!/usr/bin/env python3
"""GATE-GITENV's mutants: each fix taken out of its gate, one at a time, in a
--no-local clone (never the worktree), and scripts/test-gate-env.sh run. A
mutant is CAUGHT when the test fails and its failing cases are exactly the
ones that fix exists for.

usage: mutants.py <clone> <log dir>
"""
import os
import pathlib
import re
import subprocess
import sys

clone = pathlib.Path(sys.argv[1])
logs = pathlib.Path(sys.argv[2])
logs.mkdir(parents=True, exist_ok=True)
test = clone / "scripts/test-gate-env.sh"

SCRUB = re.compile(r"\n# Git takes where a repository is.*?\ndone\n", re.S)

mutants = {
    "no-secrets-keeps-git-env": (
        "check-no-secrets", "scrub", None,
        {"no-secrets-index-file", "no-secrets-object-dir", "no-secrets-common-dir"},
    ),
    "identity-keeps-git-env": (
        "check-xfx-identity", "scrub", None,
        {"identity-git-dir", "identity-git-dir-work-tree", "identity-index-file",
         "identity-object-dir", "identity-common-dir"},
    ),
    "preview-keeps-git-env": (
        "check-preview-contract", "scrub", None,
        {"preview-git-dir", "preview-git-dir-work-tree", "preview-object-dir", "preview-common-dir"},
    ),
    "identity-binary-by-attributes": (
        "check-xfx-identity", "edit",
        ("git -C \"$1\" grep -a --no-color -l -z -i", "git -C \"$1\" grep -I --no-color -l -z -i"),
        {"attributes-info", "attributes-local-config", "attributes-untracked"},
    ),
    "identity-coloured-names": (
        "check-xfx-identity", "edit",
        ("git -C \"$1\" grep -a --no-color -l -z -i", "git -C \"$1\" grep -a -l -z -i"),
        {"identity-color-grep"},
    ),
}

env = dict(os.environ)
for key in list(env):
    if key.startswith("GIT_") or key == "GREP_OPTIONS":
        env.pop(key)


def run(name):
    log = logs / f"{name}.log"
    with log.open("w") as out:
        status = subprocess.run(["timeout", "1500", str(test)], cwd=clone, env=env,
                                stdin=subprocess.DEVNULL, stdout=out, stderr=subprocess.STDOUT).returncode
    text = log.read_text(errors="replace")
    failing = {m.group(1) for m in re.finditer(r"^test-gate-env: ([a-z-]+): ", text, re.M)}
    return status, failing


status, failing = run("control")
print(f"control: test exit {status}; failing: {', '.join(sorted(failing)) or '-'}", flush=True)
if status != 0:
    sys.exit("the unmodified clone fails the test; no mutant below would prove anything")

caught = 0
for name, (gate, kind, edit, expected) in mutants.items():
    path = clone / f"scripts/{gate}.sh"
    original = path.read_text()
    if kind == "scrub":
        found = SCRUB.findall(original)
        if len(found) != 1:
            sys.exit(f"{name}: the GIT_* block occurs {len(found)} times, expected once")
        mutated = SCRUB.sub("\n", original, count=1)
    else:
        old, new = edit
        if original.count(old) != 1:
            sys.exit(f"{name}: {old!r} occurs {original.count(old)} times, expected once")
        mutated = original.replace(old, new)
    try:
        path.write_text(mutated)
        status, failing = run(name)
    finally:
        path.write_text(original)
    verdict = "CAUGHT" if status != 0 and failing == expected else ("SURVIVED" if status == 0 else "OTHER")
    caught += verdict == "CAUGHT"
    print(f"{verdict} {name}: test exit {status}; failing: {', '.join(sorted(failing)) or '-'}", flush=True)

print(f"{caught} of {len(mutants)} mutants caught, each by exactly its own cases")
sys.exit(0 if caught == len(mutants) else 1)
