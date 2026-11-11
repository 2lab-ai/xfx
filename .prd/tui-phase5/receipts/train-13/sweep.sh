#!/usr/bin/env bash
# Train 13's sweep (train 12's sweep.sh, unchanged but for this line; train
# 12's two changes to train 11's: every scenario unless SELECTION names a file
# of `--only` globs, and the size from SIZE):
# one binary per tree, each built in its own clone, each swept with its own
# tree's harness, scenarios and ledger.
# usage: FX=<fx v0.0.16> SHORT_TMP=<a short scratch dir> [SIZE=40x120] [SELECTION=<file>] \
#        suite-lock.sh sweep.sh <name> <tree-whose-harness-runs> <clone-to-build> <out>
# (as run: the oracle in an ignored directory of the checkout, and a short
# directory under the machine's scratch root, so tmux's socket paths fit)
set -u
name=$1 tree=$2 clone=$3 out=$4
FX=${FX:?the fx v0.0.16 binary}
PIN=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
SIZE=${SIZE:-40x120}
export FX_AUTO_UPGRADE=0 TMPDIR=${SHORT_TMP:?a short scratch directory}/sw-$name
mkdir -p "$TMPDIR"
( cd "$clone" && cargo build --locked ) > "$out.build.log" 2>&1
rc=$?
echo "# $name: build in $(basename "$clone") at $(git -C "$clone" rev-parse --short HEAD), exit $rc; xfx sha256 $(shasum -a 256 "$clone/target/debug/xfx" | cut -c1-64)"
[ "$rc" -eq 0 ] || exit "$rc"
only=()
if [ -n "${SELECTION:-}" ]; then
  while IFS= read -r line; do
    case "$line" in ''|'#'*) ;; *) only+=(--only "$line") ;; esac
  done < "$SELECTION"
fi
if [ -n "${SELECTION:-}" ]; then sel=$(basename "$SELECTION"); else sel="every scenario"; fi
echo "# size $SIZE; selection: $sel"
echo "# fx sha256 before: $(shasum -a 256 "$FX" | cut -c1-64); load $(sysctl -n vm.loadavg | awk '{print $2}')"
start=$SECONDS
bash "$tree/scripts/fxcap/sweep.sh" --fx "$FX" --pin-sha "$PIN" --xfx "$clone/target/debug/xfx" --jobs 2 --size "$SIZE" ${only[@]+"${only[@]}"} --out "$out" > "$out.log" 2>&1
rc=$?
echo "# $name: sweep exit $rc, $((SECONDS - start)) s; fx sha256 after: $(shasum -a 256 "$FX" | cut -c1-64); load $(sysctl -n vm.loadavg | awk '{print $2}')"
tail -5 "$out.log"
exit "$rc"
