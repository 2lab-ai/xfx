#!/usr/bin/env bash
# Train 15 (new; train 14 ran the same calls inline): the gate's files here
# -- gates.txt (gates_txt.py: a header, gates.sh's lines, each step log's
# last lines and every test-suite result line) and gate-counts.txt
# (gate_counts.sh) -- each through redacted.py.
# usage: gate-files.sh <gate-dir>   (its gates.out, logs/ and repo/)
set -u
G=$1
here=$(cd "$(dirname "$0")" && pwd)
h=$(mktemp)
echo "# Train 15's gate: gates.sh (train 14's, ci.yml and sweep.yml unchanged since main 753b0a72), every step of ci.yml's gate job and sweep.yml's two harness tests, in a git clone --no-local at $(git -C "$G/repo" rev-parse --short=8 HEAD), the whole script inside one hold of the suite lock (suite-lock.sh gates.sh), TMPDIR the harness scratch root, each step's exit from \$? on the line after its command." > "$h"
python3 -I -B "$here/gates_txt.py" "$h" "$G/gates.out" "$G/logs" "$here/gates.txt" "root:$G/repo=<repo>" "root:$G=<gate>"
bash "$here/gate_counts.sh" "$G/logs" | python3 -I -B "$here/redacted.py" "$here/gate-counts.txt" "$G/repo=<repo>" "$G=<gate>"
rm -f "$h"
