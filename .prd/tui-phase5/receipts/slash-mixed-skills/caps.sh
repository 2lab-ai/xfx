#!/usr/bin/env bash
# One suite-lock slot: fx, the base and the head on the paired scenario, and on
# SLASH-ARG-SKILLS' probe (its probe-mixed.cap, copied into scenarios/ for the
# run only).
# usage: suite-lock.sh caps.sh <worktree> <fx v0.0.16> <dir with xfx-base and xfx-head> <out-dir>
set -uo pipefail
W=${1:?worktree}; FX=${2:?fx}; B=${3:?binaries}; O=${4:?out dir}
rm -rf "$O"; mkdir -p "$O"
probe=$W/scripts/fxcap/scenarios/p5-slash-arg-skills-mixed.cap
cp "$W/.prd/tui-phase5/receipts/slash-arg-skills/probe-mixed.cap" "$probe"
trap 'rm -f "$probe"' EXIT
cd "$W"
for sc in p5-slash-mixed-skills p5-slash-arg-skills-mixed; do
  FX_BIN=$FX scripts/fxcap/capture.sh fx $sc 40x120 "$O/fx" > "$O/log-fx-$sc.txt" 2>&1; echo "fx $sc exit $?"
  XFX_BIN=$B/xfx-base scripts/fxcap/capture.sh xfx $sc 40x120 "$O/base" > "$O/log-base-$sc.txt" 2>&1; echo "base $sc exit $?"
  XFX_BIN=$B/xfx-head scripts/fxcap/capture.sh xfx $sc 40x120 "$O/head" > "$O/log-head-$sc.txt" 2>&1; echo "head $sc exit $?"
done
