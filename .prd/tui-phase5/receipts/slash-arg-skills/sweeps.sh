#!/usr/bin/env bash
# One suite-lock slot: the families' sweep with the base binary, then the head's.
# usage: suite-lock.sh sweeps.sh <worktree> <fx v0.0.16> <dir with xfx-base and xfx-head> <out-dir>
set -uo pipefail
W=${1:?worktree}; FX=${2:?fx}; B=${3:?binaries}; O=${4:?out dir}
# The capture job (caps.sh) copies the mixed probe into scenarios/ while it
# runs: the sweep starts once it is gone (at most 40 min), so both sweeps see
# one set.
for _ in $(seq 1 2400); do [ -e "$W/scripts/fxcap/scenarios/p5-slash-arg-skills-mixed.cap" ] || break; sleep 1; done
rm -rf "$O"; mkdir -p "$O"
cd "$W"
for side in base head; do
  start=$(date +%s)
  scripts/fxcap/sweep.sh --fx "$FX" --pin-sha 8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2 \
    --xfx "$B/xfx-$side" --size 40x120 --jobs 3 \
    --only 'slash*' --only 'p5-slash*' --only 'p5-mcp*' --only 'statusline*' --only 'permissions*' --only 'skills*' \
    --out "$O/$side" > "$O/$side.log" 2>&1
  echo "$side exit $? in $(( $(date +%s) - start ))s"
done
