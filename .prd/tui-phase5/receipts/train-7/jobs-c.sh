#!/usr/bin/env bash
# Train 7, join check 4, one hold of the suite lock: `stopnext` captured two
# more times at the head (the head sweep's capture is the first), each with
# the head's own harness and its debug build; then every osc scenario's xfx
# wires from the head sweep (osc-head/) and stopnext's two more runs
# (osc-head-run2/, -run3/) copied here, and STOP-NEXT-2's checker run over
# them against fx's wires at the same pin (the units' own, as STOP-NEXT-2
# reads them).
# usage: FX=<fx> SHORT_TMP=<short dir> suite-lock.sh jobs-c.sh <head worktree> <head clone> <head sweep out> <scratch>
set -u
H=$1 clone=$2 sweep=$3 scratch=$4
here=$(cd "$(dirname "$0")" && pwd)
PIN=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
export FX_AUTO_UPGRADE=0 TMPDIR=${SHORT_TMP:?}/osc
mkdir -p "$TMPDIR" "$scratch"
sha=$(git -C "$clone" rev-parse --short=7 HEAD)
echo "# jobs-c at $(date -u +%FT%TZ); head build $sha ($(shasum -a 256 "$clone/target/debug/xfx" | cut -c1-12)); fx $(shasum -a 256 "$FX" | cut -c1-12)"
for run in 2 3; do
  rm -rf "$scratch/stopnext-$run"
  bash "$H/scripts/fxcap/sweep.sh" --fx "$FX" --pin-sha "$PIN" --xfx "$clone/target/debug/xfx" --jobs 1 --size 40x120 \
    --only p5-lifecycle-osc-stopnext --out "$scratch/stopnext-$run" > "$scratch/stopnext-$run.log" 2>&1
  echo "stopnext run $run: sweep exit $?; $(grep -m1 'Scenarios:' "$scratch/stopnext-$run/summary.md" 2>/dev/null)"
done
copy() {  # copy <from sweep out> <to dir> [<scenario glob>]
  local from=$1 to=$2 glob=${3:-p5-lifecycle-osc*}
  rm -rf "$to"; mkdir -p "$to"
  find "$from" -name "xfx-$glob-40x120-*.wire" | while read -r f; do
    s=$(basename "$f" | sed -E 's/^xfx-(.*-40x120)-.*\.wire$/\1/')
    mkdir -p "$to/$s" && cp "$f" "$to/$s/"
  done
  echo "$(basename "$to"): $(find "$to" -name '*.wire' | wc -l | tr -d ' ') wires"
}
copy "$sweep" "$here/osc-head"
copy "$scratch/stopnext-2" "$here/osc-head-run2" "p5-lifecycle-osc-stopnext"
copy "$scratch/stopnext-3" "$here/osc-head-run3" "p5-lifecycle-osc-stopnext"
for side in osc-head osc-head-run2 osc-head-run3; do
  echo "## osc.py $side $sha"
  if [ "$side" = osc-head ]; then
    python3 -I -B "$here/../stop-next-2/osc.py" "$here" "$side" "$sha"
  else
    python3 -I -B "$here/../stop-next-2/osc.py" "$here" "$side" "$sha" p5-lifecycle-osc-stopnext
  fi
  echo "# exit $?"
done
