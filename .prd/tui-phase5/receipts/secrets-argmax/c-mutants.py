#!/usr/bin/env python3
"""SECRETS-ARGMAXc's mutants: each gate with its `unset GREP_OPTIONS` block
removed, run through scripts/test-gate-env.sh in a --no-local clone (never
the worktree). Then the plants alone: every block removed, and the test with
each case's GREP_OPTIONS value emptied, which must pass -- the planted
violations are real without the variable.

usage: mutants-c.py <clone> <log dir>

The unmodified clone must pass first. A mutant is CAUGHT if the test failed
and its failing cases are exactly the mutated gate's.
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
original_test = test.read_text()

gates = {
    "no-stubs": "check-no-stubs",
    "preview-contract": "check-preview-contract",
    "test-preview-contract": "test-preview-contract",
    "tui-preflight-cost": "check-tui-preflight-cost",
    "usage-clock": "check-usage-clock",
}
originals = {g: (clone / f"scripts/{s}.sh").read_text() for g, s in gates.items()}
BLOCK = re.compile(r"\n# BSD grep -- macOS's -- puts `GREP_OPTIONS`.*?\nunset GREP_OPTIONS\n", re.S)

env = dict(os.environ)
for key in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE", "GREP_OPTIONS"):
    env.pop(key, None)


def without_unset(gate):
    text = originals[gate]
    found = BLOCK.findall(text)
    if len(found) != 1:
        sys.exit(f"{gate}: the unset block occurs {len(found)} times, expected once")
    return BLOCK.sub("\n", text, count=1)


def run(name):
    log = logs / f"{name}.log"
    with log.open("w") as out:
        status = subprocess.run(["timeout", "1200", str(test)], cwd=clone, env=env,
                                stdin=subprocess.DEVNULL, stdout=out, stderr=subprocess.STDOUT).returncode
    text = log.read_text(errors="replace")
    failing = sorted({m.group(1) for m in re.finditer(r"^test-gate-env: ([a-z-]+): with GREP_OPTIONS", text, re.M)})
    return status, failing


def restore():
    for g, s in gates.items():
        (clone / f"scripts/{s}.sh").write_text(originals[g])
    test.write_text(original_test)


status, failing = run("control")
print(f"control: test exit {status}; failing: {', '.join(failing) or '-'}", flush=True)
if status != 0:
    sys.exit("the unmodified clone fails the test; no mutant below would prove anything")

caught = 0
try:
    for gate, script in gates.items():
        (clone / f"scripts/{script}.sh").write_text(without_unset(gate))
        status, failing = run(f"no-unset-{gate}")
        verdict = "CAUGHT" if status != 0 and failing == [gate] else ("SURVIVED" if status == 0 else "OTHER")
        caught += verdict == "CAUGHT"
        print(f"{verdict} no-unset-{gate}: test exit {status}; failing: {', '.join(failing) or '-'}", flush=True)
        restore()
    # The plants alone: no unset anywhere, and no GREP_OPTIONS value.
    for gate, script in gates.items():
        (clone / f"scripts/{script}.sh").write_text(without_unset(gate))
    emptied, n = re.subn(r"^(expect [a-z-]+) (-q|-v) ", r"\1 '' ", original_test, flags=re.M)
    if n != len(gates):
        sys.exit(f"emptied {n} cases' values, expected {len(gates)}")
    test.write_text(emptied)
    status, failing = run("plants-without-grep-options")
    print(f"plants, every unset removed, every value empty: test exit {status}; failing: {', '.join(failing) or '-'}", flush=True)
finally:
    restore()

print(f"{caught} of {len(gates)} mutants caught, each by its own case only")
sys.exit(0 if caught == len(gates) and status == 0 else 1)
