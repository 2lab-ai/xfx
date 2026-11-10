#!/usr/bin/env bash
# Train 13 (trains 9's to 12's sweep-rerun.sh, unchanged but for this line): one scenario swept again on a tree's already-built debug binary,
# with that tree's harness, scenarios and ledger, <n> times one after
# another (train 7's sweep-rerun.sh's use: a pair that moved on fx's side
# alone, captured again).
# usage: FX=<fx v0.0.16> SHORT_TMP=<a short scratch dir> \
#        suite-lock.sh sweep-rerun.sh <name> <tree> <scenario> <n> <out-prefix>
set -u
name=$1 tree=$2 scenario=$3 n=$4 out=$5
FX=${FX:?the fx v0.0.16 binary}
PIN=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
export FX_AUTO_UPGRADE=0 TMPDIR=${SHORT_TMP:?a short scratch directory}/rr-$name
mkdir -p "$TMPDIR"
for i in $(seq 1 "$n"); do
  echo "# $name rerun $i of $scenario at $(git -C "$tree" rev-parse --short HEAD); xfx sha256 $(shasum -a 256 "$tree/target/debug/xfx" | cut -c1-64); fx sha256 before $(shasum -a 256 "$FX" | cut -c1-64); load $(sysctl -n vm.loadavg | awk '{print $2}')"
  bash "$tree/scripts/fxcap/sweep.sh" --fx "$FX" --pin-sha "$PIN" --xfx "$tree/target/debug/xfx" --jobs 1 --size 40x120 --only "$scenario" --out "$out-$i" > "$out-$i.log" 2>&1
  echo "# rerun $i: exit $?; fx sha256 after $(shasum -a 256 "$FX" | cut -c1-64); $(grep -E '^pass =' "$out-$i.log")"
done
