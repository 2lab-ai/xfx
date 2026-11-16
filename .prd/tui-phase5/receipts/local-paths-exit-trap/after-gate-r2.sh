#!/usr/bin/env bash
# LOCAL-PATHS-EXIT-TRAP round 2: what needs the gate's release binary, in one
# hold of the suite lock: each changed script's normal run, round 1's head
# (ef4e74ea) against round 2's, on one tree (normal.sh).
#
#   suite-lock.sh after-gate-r2.sh <clone at the head> <gate clone> <tmp> <out>
set -u
clone=$1 gate=$2 T=$3 out=$4
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin="$gate/target/release/xfx"
[ -x "$bin" ] || { echo "no release binary from the gate"; exit 2; }
mkdir -p "$T/normal"
TMPDIR="$T/normal" /bin/bash "$here/normal.sh" "$clone" ef4e74ea "$T/normal/logs" "$bin" >"$out" 2>&1
echo "normal.sh exit $?"
