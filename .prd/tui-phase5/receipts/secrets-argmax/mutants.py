#!/usr/bin/env python3
"""Mutants of check-no-secrets.sh's batching, each run through
scripts/test-check-no-secrets.sh in a --no-local clone (never the worktree).

usage: mutants.py <clone> <log dir> [<the base check-no-secrets.sh>]
"""
import os
import pathlib
import subprocess
import sys

clone = pathlib.Path(sys.argv[1])
logs = pathlib.Path(sys.argv[2])
logs.mkdir(parents=True, exist_ok=True)
script = clone / "scripts/check-no-secrets.sh"
original = script.read_text()

GREP = 'batch_hits="$(grep -H -n -E -e "$pattern" -- "${@:$((start + 1)):$count}" 2>&1 </dev/null)" || status=$?\n'
STATUS = '\t\t\tif [ "$status" -ge 2 ]; then\n\t\t\t\tunreadable=yes\n'
APPEND_END = '\t\t\t\thits="${hits:+$hits\n}$batch_hits"\n\t\t\tfi\n\t\tdone\n'
INNER_LOOP = '\t\tfor bound in $bounds; do\n\t\t\tstart="${bound%%:*}"\n\t\t\tcount="${bound#*:}"\n\t\t\t# Status 1'
LAST_BOUND = '\t\tsize=$((size + cost))\n\tdone\n\tprintf \'%d:%d\\n\' "$start" "$count"\n}\n'
GUARD = '\tif [ "$covered" -ne "$#" ]; then\n\t\tfail "the batches cover'

mutants = {
    # 1. a batch other than the last: its status is not looked at
    "nonfinal-status-ignored": [
        (STATUS, '\t\t\tif [ "$status" -ge 2 ] && [ $((start + count)) -eq "$#" ]; then\n\t\t\t\tunreadable=yes\n'),
    ],
    # 2. a batch dropped: the second one, for every pattern
    "second-batch-dropped": [
        (INNER_LOOP, '\t\tdropped="$(printf \'%s\\n\' $bounds | sed -n 2p)"\n\t\tfor bound in $bounds; do\n\t\t\t[ "$bound" = "$dropped" ] && continue\n\t\t\tstart="${bound%%:*}"\n\t\t\tcount="${bound#*:}"\n\t\t\t# Status 1'),
    ],
    # 3. stop after the first batch
    "first-batch-only": [
        (APPEND_END, '\t\t\t\thits="${hits:+$hits\n}$batch_hits"\n\t\t\tfi\n\t\t\tbreak\n\t\tdone\n'),
    ],
    # 4. the first name of every batch falls between two
    "batch-edge-lost": [
        (GREP, GREP.replace('"${@:$((start + 1)):$count}"', '"${@:$((start + 2)):$((count - 1))}"')),
    ],
    # 5. -H dropped: a one-name batch prints its hits without the name
    "with-filename-dropped": [
        (GREP, GREP.replace("grep -H -n", "grep -n")),
    ],
    # 6. the last batch never emitted -- the coverage guard alone stops it
    "last-bound-lost": [
        (LAST_BOUND, '\t\tsize=$((size + cost))\n\tdone\n}\n'),
    ],
    # 7. the same, with the coverage guard removed too
    "last-bound-lost-unguarded": [
        (LAST_BOUND, '\t\tsize=$((size + cost))\n\tdone\n}\n'),
        (GUARD, '\tif false; then\n\t\tfail "the batches cover'),
    ],
}

env = dict(os.environ)
for key in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
    env.pop(key, None)

# The RED: the check as it is at the base, under the same test.
order = list(mutants.items())
if len(sys.argv) > 3:
    order.insert(0, ("base-script", pathlib.Path(sys.argv[3]).read_text()))

def run(name):
    log = logs / f"{name}.log"
    with log.open("w") as out:
        status = subprocess.run(
            ["timeout", "600", str(clone / "scripts/test-check-no-secrets.sh")],
            cwd=clone, env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=subprocess.STDOUT,
        ).returncode
    return status, log.read_text(errors="replace")


# The unmutated check must pass first, or every "caught" below could be the
# harness failing everything.
status, text = run("control")
print(f"control: test exit {status}", flush=True)
if status != 0:
    sys.exit("the unmutated check fails the test; no mutant below would prove anything")

results = []
try:
    for name, edits in order:
        if isinstance(edits, str):
            text = edits
        else:
            text = original
            for old, new in edits:
                n = text.count(old)
                if n != 1:
                    sys.exit(f"mutant {name}: {old!r} occurs {n} times, expected once")
                text = text.replace(old, new)
        assert text != original
        script.write_text(text)
        status, output = run(name)
        failed = [
            line.split(":", 2)[1].strip()
            for line in output.splitlines()
            if line.startswith("test-check-no-secrets: ")
            and not line.startswith("test-check-no-secrets: ok")
            and "overflow ARG_MAX" not in line
        ]
        # Caught means a case failed: the precondition held (the tree overflowed
        # ARG_MAX) and the check, not the harness, is what the test refused.
        if status == 0:
            verdict = "SURVIVED"
        elif "overflow ARG_MAX" in output and failed:
            verdict = "CAUGHT"
        else:
            verdict = "INVALID"
        results.append(f"{verdict} {name}: test exit {status}; failing: {'; '.join(failed)}")
        print(results[-1], flush=True)
finally:
    script.write_text(original)

caught = [r for r in results if r.startswith("CAUGHT")]
print(f"{len(caught)} of {len(results)} mutants caught")
sys.exit(0 if len(caught) == len(results) else 1)
