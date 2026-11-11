#!/bin/bash
# xfx's captures of AT-RANK's pairs at the head build: p5-at-rank, p5-input-at, workspace-index at 40x120.
set -u
: "${TMPDIR:?the capture root, short: the run directory is written there}"
wt=$1 out=$2
cd "$wt" || exit 2
mkdir -p "$out"
for sc in p5-at-rank p5-input-at workspace-index; do
  XFX_BIN="$wt/target/debug/xfx" scripts/fxcap/capture.sh xfx "$sc" 40x120 "$out"
  echo "capture $sc exit $?"
done
