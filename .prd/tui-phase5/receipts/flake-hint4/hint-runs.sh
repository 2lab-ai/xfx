#!/usr/bin/env bash
# Runs one scenario n times on fx and on xfx, fx first and then xfx, as the
# sweep runs a unit, and says for each run and each snap whether the snap's
# screen holds a timer's row -- $HINT, by default the full-access warning
# (`Full access enabled`) -- with the 1-minute load average when the run
# began. The default is the sentence the RED was counted with; at 120
# columns it is the warning's whole row, as `Full access` (capfile.py TIMERS,
# which also matches the narrow form) is.
#
# usage: [HINT=<text>] hint-runs.sh <fx-bin> <xfx-bin> <n> <scenario> <out-dir> [<scenarios-dir>]
#
# <out-dir> gets every run's captures (runs/<i>/) and each capture log. The
# lines printed name no path: `run <i> <bin> <label> hint=<0|1> load=<x>`, a
# `run <i> <bin>: capture exit <code>` for a run that failed, then a count per
# binary and label, and how many pairs (a run's two snaps of one label) agree.
set -euo pipefail
fx=${1:?fx binary} xfx=${2:?xfx binary} n=${3:?runs} scen=${4:?scenario} out=${5:?out dir}
here=$(cd "$(dirname "$0")/../../../.." && pwd)/scripts/fxcap
[[ -z ${6:-} ]] || export FXCAP_SCENARIOS=$6
hint=${HINT:-Full access enabled}
mkdir -p "$out"
log=$out/hint-runs.log
: > "$log"
for ((i = 1; i <= n; i++)); do
  d=$out/runs/$i
  mkdir -p "$d"
  for which in fx xfx; do
    load=$(sysctl -n vm.loadavg | awk '{print $2}')
    code=0
    if [[ $which == fx ]]; then
      FX_BIN=$fx bash "$here/capture.sh" fx "$scen" 40x120 "$d" > "$d/$which.log" 2>&1 || code=$?
    else
      XFX_BIN=$xfx bash "$here/capture.sh" xfx "$scen" 40x120 "$d" > "$d/$which.log" 2>&1 || code=$?
    fi
    (( code == 0 )) || echo "run $i $which: capture exit $code" | tee -a "$log"
    for snap in "$d/$which-$scen-40x120-"*.txt; do
      [[ -f $snap ]] || continue
      label=${snap#"$d/$which-$scen-40x120-"}
      label=${label%.txt}
      h=0
      if grep -qF -- "$hint" "$snap"; then h=1; fi
      echo "run $i $which $label hint=$h load=$load" | tee -a "$log"
    done
  done
done
awk -v n="$n" '
  $1 == "run" && $5 ~ /^hint=/ { split($5, h, "="); c[$3 " " $4] += h[2]; seen[$3 " " $4] = 1; v[$2 " " $3 " " $4] = h[2]; labels[$4] = 1; runs[$2] = 1 }
  END {
    for (k in seen) printf "count %s: hint shown in %d of %d\n", k, c[k], n | "sort"
    close("sort")
    for (l in labels) { a = 0; p = 0
      for (r in runs) if ((r " fx " l) in v && (r " xfx " l) in v) { p++; if (v[r " fx " l] == v[r " xfx " l]) a++ }
      printf "label %s: the two binaries agree in %d of %d runs\n", l, a, p | "sort" }
  }' "$log"
