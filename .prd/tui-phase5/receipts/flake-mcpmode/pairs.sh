#!/bin/bash
# usage: FX_BIN=<fx v0.0.16> XFX_BIN=<one xfx build> BASE_FXCAP=<dir> HEAD_FXCAP=<dir> \
#        pairs.sh <repo> <names-file> <out-root> <jobs>
# The same scenarios swept twice with the repo's sweep.py, the same fx and the
# same xfx build: once through BASE_FXCAP's capture.sh (a `git archive 690aa505
# scripts/fxcap` export: its capture.sh, fakegw.ts and mcphttp.ts), once through
# HEAD_FXCAP's (the head's export), each sweep under the suite lock.
# Run as: FX_BIN, XFX_BIN = the binaries; names-file = scenarios.tsv's first column.
set -u
repo=$1 names=$2 root=$3 jobs=$4
PIN=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
only=()
while IFS= read -r n; do only+=(--only "$n"); done < "$names"
export TMPDIR="${TMPDIR:?set TMPDIR to a scratch directory}/flake-mcpmode-sw"
mkdir -p "$TMPDIR"
for side in base head; do
  case $side in
    base) cap=$BASE_FXCAP/capture.sh ;;
    head) cap=$HEAD_FXCAP/capture.sh ;;
  esac
  rm -rf "$root/$side"
  echo "$side start $(date -u +%FT%TZ)"
  ( cd "$repo" && FXCAP_SWEEP_CAPTURE=$cap "${SUITE_LOCK:?set SUITE_LOCK to the suite-lock script}" \
      python3 -I scripts/fxcap/sweep.py --fx "$FX_BIN" --pin-sha "$PIN" --xfx "$XFX_BIN" --jobs "$jobs" \
      --tmp "$TMPDIR" "${only[@]}" --out "$root/$side" ) > "$root/$side.log" 2>&1
  echo "$side rc=$? end $(date -u +%FT%TZ)"
done
