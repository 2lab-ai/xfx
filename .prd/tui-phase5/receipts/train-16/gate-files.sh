#!/usr/bin/env bash
# Train 16: train 15's gate-files.sh, its header line this train's (round 3: the join's four steps named).
# Train 15 (new; train 14 ran the same calls inline): the gate's files here
# -- gates.txt (gates_txt.py: a header, gates.sh's lines, each step log's
# last lines and every test-suite result line) and gate-counts.txt
# (gate_counts.sh) -- each through redacted.py.
# Round 5: an optional <suffix> names a second gate's files (gates<suffix>.txt,
# gate-counts<suffix>.txt), and <how> says in the header how it was run.
# usage: gate-files.sh <gate-dir> [<suffix> <how>]   (its gates.out, logs/ and repo/)
set -u
G=$1 suffix=${2:-} how=${3:-}
here=$(cd "$(dirname "$0")" && pwd)
h=$(mktemp)
echo "# Train 16's gate: gates.sh (train 15's with TEST-ENV-ISOLATION's two env-check steps and, round 3, the join of main's four: the pinned toolchain, /bin/bash first on PATH, which bash, test-early-stop.sh; ci-steps.txt), every step of ci.yml's gate job and sweep.yml's two harness tests, in a git clone --no-local at $(git -C "$G/repo" rev-parse --short=8 HEAD)${how:+, $how}, the whole script inside one hold of the suite lock (suite-lock.sh gates.sh), TMPDIR the harness scratch root, each step's exit from \$? on the line after its command." > "$h"
python3 -I -B "$here/gates_txt.py" "$h" "$G/gates.out" "$G/logs" "$here/gates$suffix.txt" "root:$G/repo=<repo>" "root:$G=<gate>"
bash "$here/gate_counts.sh" "$G/logs" | python3 -I -B "$here/redacted.py" "$here/gate-counts$suffix.txt" "$G/repo=<repo>" "$G=<gate>"
rm -f "$h"
