#!/usr/bin/env bash
# Train 14, ci-08066ac9 (new): PR #93's CI Sweep regression reproduced --
# `workspace-index` `at-plan` at a run directory as wide as CI's (55: a
# 24-byte TMPDIR + `/fxsw.` and eight + `/NNN` + `/fxcap.` and six; CI's is
# RUNNER_TEMP, `/Users/<runner>/work/_temp`, 24 bytes). The TMPDIR sits
# directly under the system's /private/tmp, so, like RUNNER_TEMP, its fixed
# part holds no `l` (WORKSPACE-INDEX-AT-PLAN's receipt did the same).
# Twelve copies of the scenario a sweep (one sweep runs twelve units), three
# sweeps a tree, head and base in turn: xfx b3dffdad (the code head; the
# head sweep's clone) and xfx dcfa5146 (the base; the base sweep's clone),
# each against fx v0.0.16 (sha256 checked before and after each sweep,
# FX_AUTO_UPGRADE=0), all with the head tree's harness, as CI's sweep runs
# both builds with the PR's (the train changes capture.sh only by the
# `homemode` verb). Captures go through wrap.sh, which logs each run
# directory's name to a scratch file. Run the whole script inside one hold
# of the suite lock.
# usage: FX=<fx v0.0.16> suite-lock.sh run.sh <head-clone> <base-clone> <tmp24> <out>
set -u
head=$1 base=$2 tmp=$3 out=$4
here=$(cd "$(dirname "$0")" && pwd)
PIN=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
FX=${FX:?the fx v0.0.16 binary}
export FX_AUTO_UPGRADE=0 FXCAP_SWEEP_CAPTURE=$here/wrap.sh REAL_CAPTURE=$head/scripts/fxcap/capture.sh
mkdir -p "$tmp" "$out/scen-rep"
cp -R "$head/scripts/fxcap/scenarios/workspace-extra" "$out/scen-rep/"
for i in $(seq -w 1 12); do cp "$head/scripts/fxcap/scenarios/workspace-index.cap" "$out/scen-rep/workspace-index-$i.cap"; done
# (as run, this line used a `case` inside `$( )`, which bash refused: the
# first lines of run.txt; it now uses grep, and prints the same fact)
echo "# TMPDIR of ${#tmp} bytes, its fixed part holding an l: $(printf %s "$tmp" | grep -qi l && echo yes || echo no)"
for i in 1 2 3; do
  for t in head base; do
    if [ "$t" = head ]; then clone=$head; else clone=$base; fi
    export NAMES_LOG=$out/names-$t-$i.tsv
    : > "$NAMES_LOG"
    echo "# $t sweep $i: xfx $(git -C "$clone" rev-parse --short=8 HEAD), sha256 $(shasum -a 256 "$clone/target/debug/xfx" | cut -c1-16); fx sha256 before $(shasum -a 256 "$FX" | cut -c1-64); load $(sysctl -n vm.loadavg | awk '{print $2}')"
    bash "$head/scripts/fxcap/sweep.sh" --fx "$FX" --pin-sha "$PIN" --xfx "$clone/target/debug/xfx" --jobs 2 --size 40x120 \
      --tmp "$tmp" --scenarios "$out/scen-rep" --only 'workspace-index-*' --out "$out/$t-$i" > "$out/$t-$i.log" 2>&1
    echo "# exit $?; fx sha256 after $(shasum -a 256 "$FX" | cut -c1-64); $(grep -E '^pass =' "$out/$t-$i.log"); names logged $(wc -l < "$NAMES_LOG" | tr -d ' ')"
  done
done
