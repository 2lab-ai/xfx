#!/usr/bin/env bash
# Train 5's OSC 7501 captures: each named scenario on fx and on xfx, through
# the repository's scripts/fxcap/capture.sh from <repo>, with its scenarios read
# from <scenarios> (FXCAP_SCENARIOS: the head's scenario directory copied, with
# this receipt's twins in osc/scenarios/ added). One directory per binary and
# scenario under <out>, as osc.py reads them: <out>/<fx|xfx>/<scenario>-40x120/.
# One line per capture: the binary, the scenario, capture.sh's exit.
#
# usage: osc/capture.sh <repo> <scenarios> <out> <xfx-bin> <fx-bin> <scenario>...
set -u
repo=$1 scenarios=$2 out=$3 xfx=$4 fx=$5
shift 5
cd "$repo" || exit 2
for name in "$@"; do
  for side in fx xfx; do
    dir=$out/$side/$name-40x120
    rm -rf "$dir"
    mkdir -p "$dir"
    case $side in
      fx) FXCAP_SCENARIOS=$scenarios FX_BIN=$fx FX_AUTO_UPGRADE=0 timeout 300 \
            scripts/fxcap/capture.sh fx "$name" 40x120 "$dir" > "$dir/run.log" 2>&1 ;;
      xfx) FXCAP_SCENARIOS=$scenarios XFX_BIN=$xfx timeout 300 \
            scripts/fxcap/capture.sh xfx "$name" 40x120 "$dir" > "$dir/run.log" 2>&1 ;;
    esac
    echo "$side $name exit=$?"
  done
done
