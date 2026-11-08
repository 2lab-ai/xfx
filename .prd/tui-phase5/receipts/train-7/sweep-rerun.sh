#!/usr/bin/env bash
# The head sweep's moved and row-gaining pairs, swept again on the same head
# build once another worker's load (16:35-16:43Z, about 29-37) was gone:
# the scenarios given, the head's harness, one hold of the suite lock.
# usage: FX=<fx> SHORT_TMP=<dir> suite-lock.sh sweep-rerun.sh <head worktree> <head clone> <out> <scenario>...
set -u
H=$1 clone=$2 out=$3; shift 3
PIN=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
export FX_AUTO_UPGRADE=0 TMPDIR=${SHORT_TMP:?}/rerun
mkdir -p "$TMPDIR"
only=(); for s in "$@"; do only+=(--only "$s"); done
echo "# rerun at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}'); xfx $(git -C "$clone" rev-parse --short HEAD) sha256 $(shasum -a 256 "$clone/target/debug/xfx" | cut -c1-12)"
bash "$H/scripts/fxcap/sweep.sh" --fx "$FX" --pin-sha "$PIN" --xfx "$clone/target/debug/xfx" --jobs 2 --size 40x120 "${only[@]}" --out "$out" > "$out.log" 2>&1
echo "# sweep exit $?, load after $(sysctl -n vm.loadavg | awk '{print $2}')"
tail -2 "$out.log"
