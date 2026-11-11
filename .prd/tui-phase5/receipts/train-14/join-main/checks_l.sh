#!/usr/bin/env bash
# Train 14's join of main (new): checks.sh's twelve copies of
# `workspace-index` once more, at a 24-byte TMPDIR whose fixed part holds an
# `l` -- since WORKSPACE-INDEX-AT-PLAN the sweep's root and each run
# directory are named by digits (`sweep.py` `digit_dir`, `capture.sh`
# `make_run`), so `@plan`'s row 13 is there or not by the TMPDIR alone, and
# checks.sh's two TMPDIRs, like CI's RUNNER_TEMP, hold none: every unit there
# lists the folder on neither side. Here it is listed, on both. The same
# sweep command, the same head clone and build as checks.sh's; fx v0.0.16's
# sha256 checked before and after, FX_AUTO_UPGRADE=0.
# usage: FX=<fx v0.0.16> suite-lock.sh checks_l.sh <sweep-clone> <tmp24l> <out>
set -u
sc=$1 tl=$2 out=$3
PIN=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
FX=${FX:?the fx v0.0.16 binary}
export FX_AUTO_UPGRADE=0
mkdir -p "$tl"
echo "# sweep clone at $(git -C "$sc" rev-parse --short=8 HEAD), tree clean: $([ -z "$(git -C "$sc" status --porcelain)" ] && echo yes || echo no)"
echo "# rep24l: TMPDIR of ${#tl} bytes, holding an l: $(printf %s "$tl" | grep -qi l && echo yes || echo no); fx sha256 before $(shasum -a 256 "$FX" | cut -c1-64); load $(sysctl -n vm.loadavg | awk '{print $2}')"
bash "$sc/scripts/fxcap/sweep.sh" --fx "$FX" --pin-sha "$PIN" --xfx "$sc/target/debug/xfx" --jobs 2 --size 40x120 \
  --tmp "$tl" --scenarios "$out/scen-rep" --only 'workspace-index-*' --out "$out/rep24l" > "$out/rep24l.log" 2>&1
echo "# rep24l: exit $?; fx sha256 after $(shasum -a 256 "$FX" | cut -c1-64); $(grep -E '^pass =' "$out/rep24l.log")"
