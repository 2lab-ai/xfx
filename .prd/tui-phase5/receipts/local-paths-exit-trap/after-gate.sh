#!/usr/bin/env bash
# LOCAL-PATHS-EXIT-TRAP: what needs the gate's release binary, in one hold of
# the suite lock: each changed script's normal run at the base and the head
# (normal.sh), and smoke.sh's `finished=1` mutant, whose killer is
# smoke.sh's own normal run.
#
#   suite-lock.sh after-gate.sh <scratch> <tmp>
set -u
S=$1 T=$2
bin="$S/clone-gate/target/release/xfx"
[ -x "$bin" ] || { echo "no release binary from the gate"; exit 2; }
mkdir -p "$T/normal" "$T/m6"
TMPDIR="$T/normal" "$S/normal.sh" "$S/clone-head32" dcfa5146 "$T/normal/logs" "$bin" > "$S/normal.txt" 2>&1
echo "normal.sh exit $?"
TMPDIR="$T/m6" XFX_SMOKE_BINARY="$bin" python3 -I "$S/mutants.py" "$S/clone-m6" rest M6 > "$S/mutants-m6.txt" 2>&1
echo "mutants M6 exit $?"
