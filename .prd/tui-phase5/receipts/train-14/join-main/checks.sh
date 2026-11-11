#!/usr/bin/env bash
# Train 14's join of main (new): the checks after the join, in one hold of
# the suite lock -- in a `git clone --no-local` at <head>, `cargo test
# --locked` and the two harness suites the join touches
# (scripts/fxcap/test-sweep.sh, test-capture.sh), each exit from `$?` on the
# line after its command; then, in another clone at <head>, its debug
# build, and `workspace-index` swept with the head's harness -- the scenario
# as the Sweep selects it, three times at a 24-byte TMPDIR and three times
# at a 49-byte one, then twelve copies of it at each -- each TMPDIR directly
# under the system's /private/tmp, its fixed part holding no `l`, as CI's
# RUNNER_TEMP (ci-08066ac9/README.md). fx v0.0.16's sha256 is checked
# before and after each sweep, FX_AUTO_UPGRADE=0.
# usage: FX=<fx v0.0.16> suite-lock.sh checks.sh <test-clone> <sweep-clone> <head> <tmp24> <tmp49> <out>
set -u
tc=$1 sc=$2 head=$3 t24=$4 t49=$5 out=$6
PIN=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
FX=${FX:?the fx v0.0.16 binary}
export FX_AUTO_UPGRADE=0
mkdir -p "$out" "$t24" "$t49"
step() { # <name> <dir> <command...>
  local name=$1 dir=$2; shift 2
  local start=$SECONDS load
  load=$(sysctl -n vm.loadavg | awk '{print $2}')
  (cd "$dir" && "$@") > "$out/$name.log" 2>&1
  local rc=$?
  echo "$name: exit $rc, $((SECONDS - start)) s, load $load"
}
for c in "$tc" "$sc"; do git -C "$c" fetch -q origin && git -C "$c" checkout -q --detach "$head"; done
echo "# test clone at $(git -C "$tc" rev-parse --short=8 HEAD), tree clean: $([ -z "$(git -C "$tc" status --porcelain)" ] && echo yes || echo no)"
step cargo-test "$tc" cargo test --locked --no-fail-fast
step test-sweep "$tc" ./scripts/fxcap/test-sweep.sh
step test-capture "$tc" ./scripts/fxcap/test-capture.sh
echo "# sweep clone at $(git -C "$sc" rev-parse --short=8 HEAD)"
step build "$sc" cargo build --locked
mkdir -p "$out/scen-rep"
cp -R "$sc/scripts/fxcap/scenarios/workspace-extra" "$out/scen-rep/"
for i in $(seq -w 1 12); do cp "$sc/scripts/fxcap/scenarios/workspace-index.cap" "$out/scen-rep/workspace-index-$i.cap"; done
sweep() { # <name> <tmp> <scenarios> <only>
  echo "# $1: TMPDIR of ${#2} bytes, holding an l: $(printf %s "$2" | grep -qi l && echo yes || echo no); fx sha256 before $(shasum -a 256 "$FX" | cut -c1-64); load $(sysctl -n vm.loadavg | awk '{print $2}')"
  bash "$sc/scripts/fxcap/sweep.sh" --fx "$FX" --pin-sha "$PIN" --xfx "$sc/target/debug/xfx" --jobs 2 --size 40x120 \
    --tmp "$2" --scenarios "$3" --only "$4" --out "$out/$1" > "$out/$1.log" 2>&1
  echo "# $1: exit $?; fx sha256 after $(shasum -a 256 "$FX" | cut -c1-64); $(grep -E '^pass =' "$out/$1.log")"
}
for i in 1 2 3; do
  sweep "wi24-$i" "$t24" "$sc/scripts/fxcap/scenarios" workspace-index
  sweep "wi49-$i" "$t49" "$sc/scripts/fxcap/scenarios" workspace-index
done
sweep rep24 "$t24" "$out/scen-rep" 'workspace-index-*'
sweep rep49 "$t49" "$out/scen-rep" 'workspace-index-*'
