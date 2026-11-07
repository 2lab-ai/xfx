#!/usr/bin/env python3
"""GATE-GITENV's sweep mutants: each python fix taken out, in a --no-local
clone (never the worktree), and scripts/fxcap/test-sweep.sh run. CAUGHT when
the suite fails and the failing tests are exactly the one that fix exists for.

usage: mutants-py.py <clone> <log dir>
"""
import os
import pathlib
import re
import subprocess
import sys

clone = pathlib.Path(sys.argv[1])
logs = pathlib.Path(sys.argv[2])
logs.mkdir(parents=True, exist_ok=True)

mutants = {
    "ci-sweep-keeps-git-env": (
        "scripts/fxcap/ci_sweep.py",
        '    r = subprocess.run(["git", "-C", repo] + list(args), capture_output=True, env=env)\n',
        '    r = subprocess.run(["git", "-C", repo] + list(args), capture_output=True)\n',
        {"test_the_selection_reads_this_checkout_whatever_git_variables_it_inherits"},
    ),
    "sweep-out-guard-keeps-git-env": (
        "scripts/fxcap/sweep.py",
        "            check=True,\n            env=git_env(),\n        ).stdout.strip()\n        return os.path.realpath",
        "            check=True,\n        ).stdout.strip()\n        return os.path.realpath",
        {"test_the_out_guard_finds_this_checkout_whatever_git_variables_it_inherits"},
    ),
}

env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def run(name):
    log = logs / f"{name}.log"
    with log.open("w") as out:
        status = subprocess.run(["timeout", "900", "bash", "scripts/fxcap/test-sweep.sh"], cwd=clone, env=env,
                                stdin=subprocess.DEVNULL, stdout=out, stderr=subprocess.STDOUT).returncode
    text = log.read_text(errors="replace")
    failing = set(re.findall(r"^(?:FAIL|ERROR): (test_[a-z_]+)", text, re.M))
    return status, failing


status, failing = run("control")
print(f"control: test-sweep exit {status}; failing: {', '.join(sorted(failing)) or '-'}", flush=True)
if status != 0:
    sys.exit("the unmodified clone fails test-sweep; no mutant below would prove anything")

caught = 0
for name, (rel, old, new, expected) in mutants.items():
    path = clone / rel
    original = path.read_text()
    if original.count(old) != 1:
        sys.exit(f"{name}: {old!r} occurs {original.count(old)} times, expected once")
    try:
        path.write_text(original.replace(old, new))
        status, failing = run(name)
    finally:
        path.write_text(original)
    verdict = "CAUGHT" if status != 0 and failing == expected else ("SURVIVED" if status == 0 else "OTHER")
    caught += verdict == "CAUGHT"
    print(f"{verdict} {name}: test-sweep exit {status}; failing: {', '.join(sorted(failing)) or '-'}", flush=True)

print(f"{caught} of {len(mutants)} mutants caught, each by exactly its own test")
sys.exit(0 if caught == len(mutants) else 1)
