#!/usr/bin/env bash
# Sweeps the eight scenarios that used `modelsdown`, N times over, one sweep
# after another, each into <out-root>/run-<i>: the RED (the checkout at the
# base, the old wall-clock outage) and the GREEN (the head, the outage a
# scenario line ends) are the same command over a different checkout and
# binary. Run under the machine's suite lock; TMPDIR short (tmux sockets).
#
# usage: sweeps.sh <checkout> <fx-bin> <xfx-bin> <out-root> <n>
set -euo pipefail
checkout=$1 fx=$2 xfx=$3 root=$4 n=$5
pin=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
scenarios=(model-column-idle model-column-reask model-reask model-parked model-floor
  p5-effort-fork-effort-down p5-input-model-fail p5-stop-in-catalog-wait)
only=()
for s in "${scenarios[@]}"; do only+=(--only "$s"); done
for i in $(seq 1 "$n"); do
  out=$root/run-$i
  rm -rf "$out"
  echo "run $i: $(date -u +%H:%M:%S) load $(sysctl -n vm.loadavg)"
  "$checkout/scripts/fxcap/sweep.sh" --fx "$fx" --pin-sha "$pin" --xfx "$xfx" "${only[@]}" --out "$out" > "$root/run-$i.log" 2>&1 || echo "run $i: sweep exit $?"
  echo "run $i: done $(date -u +%H:%M:%S) load $(sysctl -n vm.loadavg)"
done
