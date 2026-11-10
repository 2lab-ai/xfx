#!/usr/bin/env bash
# Every command this receipt's figures rest on, as run, with the scratch
# directory written as $SCRATCH, the checkout as $XFX, the machine's suite
# lock as $SUITE_LOCK and the gate's TMPDIR as $GATE_TMP (each taken from the
# environment). The CI-like TMPDIRs sit directly under the system's
# /private/tmp, so their fixed part holds no `l` after its `p`, as the
# runner's RUNNER_TEMP (`/Users/<runner>/work/_temp`: a `p` in `_temp`, no `l`) does not: TMP24 is 24
# bytes (RUNNER_TEMP's length: the run directory is then 55 bytes, the
# `<scratch>` width CI's report shows), TMP49 is 49 bytes (the length the
# brief names, the capture tests' TMPDIR on the runner), TMP16 a short one.
# GATE25 is a 25-byte one under the gate's own TMPDIR, whose fixed part does
# hold `p`, `l` and `a` (`private`, `claude`): the local case.
#
# usage: SCRATCH=<dir> XFX=<checkout> FX_BIN=<fx v0.0.16> SUITE_LOCK=<suite-lock.sh> GATE_TMP=<dir> runs.sh <step>
set -euo pipefail
: "${SCRATCH:?}" "${XFX:?}"
FX=${FX_BIN:?}   # the fx v0.0.16 oracle, sha256 below
SHA=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
LOCK=${SUITE_LOCK:?}
SYS=/private/tmp
TMP24=$SYS/wia-ci-temp
TMP49=$SYS/wia-ci-temp-0123456789abcdefghijkmno
TMP16=$SYS/wia
GATE25=${GATE_TMP:?}/w

# One sweep: <harness checkout> <scenario dir> <only-glob,...> <xfx binary> <tmp> <out>.
sweep() {
  local h=$1 scen=$2 globs=$3 bin=$4 tmp=$5 out=$6 only=() g
  IFS=, read -ra gs <<< "$globs"
  for g in "${gs[@]}"; do only+=(--only "$g"); done
  mkdir -p "$tmp"
  FX_AUTO_UPGRADE=0 bash "$h/scripts/fxcap/sweep.sh" --fx "$FX" --pin-sha "$SHA" --xfx "$bin" --out "$out" \
    --jobs 2 --tmp "$tmp" --scenarios "$scen" "${only[@]}"
}

case ${1:?step} in
  ci)
    # The four Sweep runs that selected workspace-index, and their pairs.
    for r in 42762620303 42804085569 42826560730 42844785959; do
      gh run download "$r" --repo 2lab-ai/xfx -D "$SCRATCH/ci-runs/$r"
      for f in "$SCRATCH/ci-runs/$r"/*/result.json; do
        python3 -I -c '
import json, sys
d = json.load(open(sys.argv[1]))
for p in d["pairs"]:
    if p["scenario"] == "workspace-index":
        print("run %s head %s base %s  %s/%s  head=%s (runs %d) base=%s  first=%s" % (sys.argv[2], d["head"][:8], d["base"][:8],
              p["scenario"], p["label"], p["head"], p["runs"], p["base"], json.dumps(p["first"], ensure_ascii=False)))' "$f" "$r"
      done
    done
    git -C "$XFX" rev-parse '208a8dd4^{tree}' 'd2b2ca75^{tree}'
    git -C "$XFX" log -1 --format=%P d2b2ca75
    gh run view 42844785959 --repo 2lab-ai/xfx --log | grep -m2 '_temp/base\|--tmp' ;;
  clones)
    # The unfixed harness (d2b2ca75) and the base (8f921425), each a clone
    # of its own, and their binaries; the fix's binary is built at 2a721846.
    git clone --no-local -q "$XFX" "$SCRATCH/head-clone"; git -C "$SCRATCH/head-clone" checkout -q -b red-d2b2ca75 d2b2ca75
    git clone --no-local -q "$XFX" "$SCRATCH/base-clone"; git -C "$SCRATCH/base-clone" checkout -q -b base-8f921425 8f921425
    (cd "$SCRATCH/base-clone" && cargo build --locked)
    # xfx d2b2ca75 is the worktree's debug build before the fix commit, and
    # xfx 2a721846 the same worktree's build after it (row 1 names each).
    mkdir -p "$SCRATCH/xbin"
    cp "$SCRATCH/base-clone/target/debug/xfx" "$SCRATCH/xbin/xfx-8f921425" ;;
  copies)
    # Twelve copies of workspace-index, so one sweep runs twelve units.
    mkdir -p "$SCRATCH/scen-rep"
    cp -R "$XFX/scripts/fxcap/scenarios/workspace-extra" "$SCRATCH/scen-rep/"
    for i in $(seq -w 1 12); do cp "$XFX/scripts/fxcap/scenarios/workspace-index.cap" "$SCRATCH/scen-rep/workspace-index-$i.cap"; done ;;
  red)
    # The unfixed harness, twelve units a sweep.
    H=$SCRATCH/head-clone
    $LOCK bash -c "
      $(declare -f sweep); FX=$FX SHA=$SHA
      sweep $H $SCRATCH/scen-rep 'workspace-index-*' $SCRATCH/xbin/xfx-d2b2ca75 $TMP24 $SCRATCH/sw/red-d2b2ca75-24
      sweep $H $SCRATCH/scen-rep 'workspace-index-*' $SCRATCH/xbin/xfx-8f921425 $TMP24 $SCRATCH/sw/red-8f921425-24
      sweep $H $SCRATCH/scen-rep 'workspace-index-*' $SCRATCH/xbin/xfx-d2b2ca75 $TMP49 $SCRATCH/sw/red-d2b2ca75-49
      sweep $H $SCRATCH/scen-rep 'workspace-index-*' $SCRATCH/xbin/xfx-d2b2ca75 $TMP16 $SCRATCH/sw/red-d2b2ca75-16
      sweep $H $SCRATCH/scen-rep 'workspace-index-*' $SCRATCH/xbin/xfx-d2b2ca75 $GATE25 $SCRATCH/sw/red-d2b2ca75-25" ;;
  green)
    # The fixed harness (this branch at 2a721846), the same legs; then the
    # look-ahead families, head and base, at the 24-byte TMPDIR.
    H=$XFX/.worktrees/fix-p5-workspace-index-at-plan
    $LOCK bash -c "
      $(declare -f sweep); FX=$FX SHA=$SHA
      sweep $H $SCRATCH/scen-rep 'workspace-index-*' $SCRATCH/xbin/xfx-2a721846 $TMP24 $SCRATCH/sw/green-2a721846-24
      sweep $H $SCRATCH/scen-rep 'workspace-index-*' $SCRATCH/xbin/xfx-8f921425 $TMP24 $SCRATCH/sw/green-8f921425-24
      sweep $H $SCRATCH/scen-rep 'workspace-index-*' $SCRATCH/xbin/xfx-2a721846 $TMP49 $SCRATCH/sw/green-2a721846-49
      sweep $H $SCRATCH/scen-rep 'workspace-index-*' $SCRATCH/xbin/xfx-2a721846 $TMP16 $SCRATCH/sw/green-2a721846-16
      sweep $H $SCRATCH/scen-rep 'workspace-index-*' $SCRATCH/xbin/xfx-2a721846 $GATE25 $SCRATCH/sw/green-2a721846-25
      for b in 2a721846 8f921425; do
        sweep $H $H/scripts/fxcap/scenarios 'workspace*,filepicker*,p5-input-at*,p5-input-row-limit*' $SCRATCH/xbin/xfx-\$b $TMP24 $SCRATCH/sw/look-fixed-\$b-24
      done" ;;
  summary)
    for d in "$SCRATCH"/sw/*/; do echo "## ${d%/}"; python3 -I "$(dirname "$0")/pairs_summary.py" "$d"; done ;;
  tests-red)
    # The new harness tests, copied into a clone at d2b2ca75.
    git clone --no-local -q "$XFX" "$SCRATCH/redtest"; git -C "$SCRATCH/redtest" checkout -q -b redtest d2b2ca75
    cp "$XFX/.worktrees/fix-p5-workspace-index-at-plan/scripts/fxcap/test_capture.py" \
       "$XFX/.worktrees/fix-p5-workspace-index-at-plan/scripts/fxcap/test_sweep.py" "$SCRATCH/redtest/scripts/fxcap/"
    cd "$SCRATCH/redtest"
    TMPDIR=$GATE_TMP python3 -I scripts/fxcap/test_capture.py RunDirectory || true
    TMPDIR=$GATE_TMP python3 -I scripts/fxcap/test_sweep.py Sweep.test_a_units_two_runs_share_one_run_directory_named_by_digits || true ;;
  *) echo "no step $1" >&2; exit 2 ;;
esac
