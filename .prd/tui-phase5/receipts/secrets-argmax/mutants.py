#!/usr/bin/env python3
"""Mutants of check-no-secrets.sh, each run through
scripts/test-check-no-secrets.sh in a --no-local clone (never the worktree).

usage: mutants.py <clone> <log dir> <base rev> <round-1 rev>

  <base rev>     the check before the fix (the RED);
  <round-1 rev>  the commit whose check and test the first review saw: its
                 check is run under this test, and two mutants are run under
                 its test as well, to show what this round's test adds.

The unmutated clone must pass first. A mutant is CAUGHT only if the test's
precondition held (the tree overflowed ARG_MAX) and a case failed; SURVIVED
if the test passed; INVALID otherwise.
"""
import os
import pathlib
import subprocess
import sys

clone = pathlib.Path(sys.argv[1])
logs = pathlib.Path(sys.argv[2])
base_rev, round1_rev = sys.argv[3], sys.argv[4]
logs.mkdir(parents=True, exist_ok=True)
script = clone / "scripts/check-no-secrets.sh"
test = clone / "scripts/test-check-no-secrets.sh"
original = script.read_text()
original_test = test.read_text()


def show(rev, path):
    return subprocess.run(["git", "-C", str(clone), "show", f"{rev}:{path}"],
                          check=True, capture_output=True, text=True).stdout


base_check = show(base_rev, "scripts/check-no-secrets.sh")
round1_check = show(round1_rev, "scripts/check-no-secrets.sh")
round1_test = show(round1_rev, "scripts/test-check-no-secrets.sh")

GREP = 'batch_hits="$(grep -H -n -E -e "$pattern" -- "${@:$((start + 1)):$count}" 2>&1 </dev/null)" || status=$?\n'
STATUS = '\t\t\tif [ "$status" -ge 2 ]; then\n\t\t\t\tunreadable=yes\n'
APPEND_END = '\t\t\t\thits="${hits:+$hits\n}$batch_hits"\n\t\t\tfi\n\t\tdone\n'
INNER_LOOP = '\t\tfor bound in $bounds; do\n\t\t\tstart="${bound%%:*}"\n\t\t\tcount="${bound#*:}"\n\t\t\t# Status 1'
LAST_BOUND = '\t\tsize=$((size + cost))\n\tdone\n\tprintf \'%d:%d\\n\' "$start" "$count"\n}\n'
GUARD = '\tif [ "$covered" -ne "$#" ]; then\n\t\tfail "the batches cover'
UNSET = '\nunset GREP_OPTIONS\n'

mutants = {
    # round 1
    "nonfinal-status-ignored": [
        (STATUS, '\t\t\tif [ "$status" -ge 2 ] && [ $((start + count)) -eq "$#" ]; then\n\t\t\t\tunreadable=yes\n'),
    ],
    "second-batch-dropped": [
        (INNER_LOOP, '\t\tdropped="$(printf \'%s\\n\' $bounds | sed -n 2p)"\n\t\tfor bound in $bounds; do\n\t\t\t[ "$bound" = "$dropped" ] && continue\n\t\t\tstart="${bound%%:*}"\n\t\t\tcount="${bound#*:}"\n\t\t\t# Status 1'),
    ],
    "first-batch-only": [
        (APPEND_END, '\t\t\t\thits="${hits:+$hits\n}$batch_hits"\n\t\t\tfi\n\t\t\tbreak\n\t\tdone\n'),
    ],
    "batch-edge-lost": [
        (GREP, GREP.replace('"${@:$((start + 1)):$count}"', '"${@:$((start + 2)):$((count - 1))}"')),
    ],
    "with-filename-dropped": [
        (GREP, GREP.replace("grep -H -n", "grep -n")),
    ],
    "last-bound-lost": [
        (LAST_BOUND, '\t\tsize=$((size + cost))\n\tdone\n}\n'),
    ],
    "last-bound-lost-unguarded": [
        (LAST_BOUND, '\t\tsize=$((size + cost))\n\tdone\n}\n'),
        (GUARD, '\tif false; then\n\t\tfail "the batches cover'),
    ],
    # round 2
    # the third batch read in place of... the second, read twice: same count
    "third-batch-is-second": [
        (INNER_LOOP, '\t\tsecond="$(printf \'%s\\n\' $bounds | sed -n 2p)"\n\t\tthird="$(printf \'%s\\n\' $bounds | sed -n 3p)"\n\t\tfor bound in $bounds; do\n\t\t\t[ "$bound" = "$third" ] && bound="$second"\n\t\t\tstart="${bound%%:*}"\n\t\t\tcount="${bound#*:}"\n\t\t\t# Status 1'),
    ],
    # the half-revert: only grep's own 2 is a read failure
    "status-eq-2": [
        (STATUS, STATUS.replace('-ge 2', '-eq 2')),
    ],
    # `GREP_OPTIONS` reaches grep (BSD grep reads it; GNU 3.8 does not)
    "grep-options-kept": [
        (UNSET, '\n'),
    ],
    # the first batch grep could not read ends the pattern's scan
    "break-after-unreadable": [
        (STATUS, '\t\t\tif [ "$status" -ge 2 ]; then\n\t\t\t\tunreadable=yes\n\t\t\t\tbreak\n'),
    ],
    # the slice unquoted: a name is split on the tab and the newline in it
    "unquoted-slice": [
        (GREP, GREP.replace('"${@:$((start + 1)):$count}"', '${@:$((start + 1)):$count}')),
    ],
}

env = dict(os.environ)
for key in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE", "GREP_OPTIONS"):
    env.pop(key, None)


def run(name):
    log = logs / f"{name}.log"
    with log.open("w") as out:
        status = subprocess.run(
            ["timeout", "900", str(test)],
            cwd=clone, env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=subprocess.STDOUT,
        ).returncode
    return status, log.read_text(errors="replace")


def judge(name, status, output):
    failed = [
        line.split(":", 2)[1].strip()
        for line in output.splitlines()
        if line.startswith("test-check-no-secrets: ")
        and not line.startswith("test-check-no-secrets: ok")
        and "overflow ARG_MAX" not in line
        and "names carry" not in line
    ]
    if status == 0:
        verdict = "SURVIVED"
    elif "overflow ARG_MAX" in output and failed:
        verdict = "CAUGHT"
    else:
        verdict = "INVALID"
    line = f"{verdict} {name}: test exit {status}; failing: {'; '.join(failed)}"
    print(line, flush=True)
    return verdict


def mutate(name, edits):
    text = original
    for old, new in edits:
        n = text.count(old)
        if n != 1:
            sys.exit(f"mutant {name}: {old!r} occurs {n} times, expected once")
        text = text.replace(old, new)
    assert text != original
    return text


status, output = run("control")
print(f"control: test exit {status}", flush=True)
if status != 0:
    sys.exit("the unmutated check fails the test; no mutant below would prove anything")

verdicts = {}
try:
    # The RED, and the round-1 check, under this test.
    for name, text in (("base-check", base_check), ("round1-check", round1_check)):
        script.write_text(text)
        verdicts[name] = judge(name, *run(name))
    for name, edits in mutants.items():
        script.write_text(mutate(name, edits))
        verdicts[name] = judge(name, *run(name))
    # What this round's test adds: two mutants under the round-1 test.
    test.write_text(round1_test)
    for name in ("third-batch-is-second", "status-eq-2"):
        script.write_text(mutate(name, mutants[name]))
        print("under the round-1 test: ", end="", flush=True)
        judge(name, *run(f"round1-test-{name}"))
finally:
    script.write_text(original)
    test.write_text(original_test)

caught = [n for n in mutants if verdicts[n] == "CAUGHT"]
print(f"{len(caught)} of {len(mutants)} mutants caught by this test; "
      f"base-check {verdicts['base-check']}, round1-check {verdicts['round1-check']}")
sys.exit(0 if len(caught) == len(mutants) and verdicts["base-check"] == "CAUGHT" else 1)
