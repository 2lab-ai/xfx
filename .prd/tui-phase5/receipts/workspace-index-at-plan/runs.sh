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
  r2)
    # Round 2: the pair again with f718e3c1's harness, four units a leg, and
    # the new tests at cc913145 (RED), the mutants and the gate.
    git clone --no-local -q "$XFX" "$SCRATCH/h2"; git -C "$SCRATCH/h2" checkout -q -b h2 f718e3c1
    mkdir -p "$SCRATCH/scen-rep4"; cp -R "$SCRATCH/scen-rep/workspace-extra" "$SCRATCH/scen-rep4/"
    for i in 01 02 03 04; do cp "$SCRATCH/scen-rep/workspace-index-01.cap" "$SCRATCH/scen-rep4/workspace-index-$i.cap"; done
    H=$SCRATCH/h2
    $LOCK bash -c "
      $(declare -f sweep); FX=$FX SHA=$SHA
      sweep $H $SCRATCH/scen-rep4 'workspace-index-*' $SCRATCH/xbin/xfx-f718e3c1 $TMP24 $SCRATCH/sw/r2-f718e3c1-24
      sweep $H $SCRATCH/scen-rep4 'workspace-index-*' $SCRATCH/xbin/xfx-f718e3c1 $TMP49 $SCRATCH/sw/r2-f718e3c1-49"
    git clone --no-local -q "$XFX" "$SCRATCH/red2"; git -C "$SCRATCH/red2" checkout -q -b red2 cc913145
    git -C "$XFX" show f718e3c1:scripts/fxcap/test_capture.py > "$SCRATCH/red2/scripts/fxcap/test_capture.py"
    git -C "$XFX" show f718e3c1:scripts/fxcap/test_sweep.py > "$SCRATCH/red2/scripts/fxcap/test_sweep.py"
    (cd "$SCRATCH/red2" && TMPDIR=$GATE_TMP python3 -I scripts/fxcap/test_capture.py RunDirectory || true
     TMPDIR=$GATE_TMP python3 -I scripts/fxcap/test_sweep.py Sweep.test_a_units_second_run_finds_nothing_its_first_left || true)
    git clone --no-local -q "$XFX" "$SCRATCH/mut2"; git -C "$SCRATCH/mut2" checkout -q -b mut2 f718e3c1
    SCRATCH=$SCRATCH MUT_TMP=$GATE_TMP/mut2 $LOCK "$(dirname "$0")/mutants2.sh" ;;
  r3)
    # Round 3: the pair with edba4758's harness, the new tests at bf0ed67f
    # (RED), the mutants. The gate is gates.sh on a clone at edba4758.
    git clone --no-local -q "$XFX" "$SCRATCH/h3"; git -C "$SCRATCH/h3" checkout -q -b h3 edba4758
    H=$SCRATCH/h3
    $LOCK bash -c "
      $(declare -f sweep); FX=$FX SHA=$SHA
      sweep $H $SCRATCH/scen-rep4 'workspace-index-*' $SCRATCH/xbin/xfx-edba4758 $TMP24 $SCRATCH/sw/r3-edba4758-24
      sweep $H $SCRATCH/scen-rep4 'workspace-index-*' $SCRATCH/xbin/xfx-edba4758 $TMP49 $SCRATCH/sw/r3-edba4758-49"
    git clone --no-local -q "$XFX" "$SCRATCH/red3"; git -C "$SCRATCH/red3" checkout -q -b red3 bf0ed67f
    git -C "$XFX" show 87219db1:scripts/fxcap/test_capture.py > "$SCRATCH/red3/scripts/fxcap/test_capture.py"
    git -C "$XFX" show 87219db1:scripts/fxcap/test_sweep.py > "$SCRATCH/red3/scripts/fxcap/test_sweep.py"
    (cd "$SCRATCH/red3" && TMPDIR=$GATE_TMP python3 -I scripts/fxcap/test_capture.py RunDirectory || true
     TMPDIR=$GATE_TMP python3 -I scripts/fxcap/test_sweep.py Sweep.test_a_run_that_failed_is_a_failed_unit_and_its_path_is_not_reused || true)
    git clone --no-local -q "$XFX" "$SCRATCH/mut3"; git -C "$SCRATCH/mut3" checkout -q -b mut3 edba4758
    SCRATCH=$SCRATCH MUT_TMP=$GATE_TMP/mut3 $LOCK "$(dirname "$0")/mutants3.sh" ;;
  r4)
    # Round 4: the pair with 2e85d15b's harness, eight units a leg, the new
    # tests at a7dbc9d0 (RED), the mutants. The gate is gates.sh on a clone at 2e85d15b.
    git clone --no-local -q "$XFX" "$SCRATCH/h4"; git -C "$SCRATCH/h4" checkout -q -b h4 2e85d15b
    mkdir -p "$SCRATCH/scen-rep8"; cp -R "$SCRATCH/scen-rep/workspace-extra" "$SCRATCH/scen-rep8/"
    for i in 01 02 03 04 05 06 07 08; do cp "$SCRATCH/scen-rep/workspace-index-01.cap" "$SCRATCH/scen-rep8/workspace-index-$i.cap"; done
    H=$SCRATCH/h4
    $LOCK bash -c "
      $(declare -f sweep); FX=$FX SHA=$SHA
      sweep $H $SCRATCH/scen-rep8 'workspace-index-*' $SCRATCH/xbin/xfx-2e85d15b $TMP24 $SCRATCH/sw/r4-2e85d15b-24
      sweep $H $SCRATCH/scen-rep8 'workspace-index-*' $SCRATCH/xbin/xfx-2e85d15b $TMP49 $SCRATCH/sw/r4-2e85d15b-49"
    git clone --no-local -q "$XFX" "$SCRATCH/red4"; git -C "$SCRATCH/red4" checkout -q -b red4 a7dbc9d0
    git -C "$XFX" show 2e85d15b:scripts/fxcap/test_capture.py > "$SCRATCH/red4/scripts/fxcap/test_capture.py"
    (cd "$SCRATCH/red4" && TMPDIR=$GATE_TMP python3 -I scripts/fxcap/test_capture.py \
       RunDirectory.test_a_pane_pid_that_cannot_be_read_fails_the_run RunDirectory.test_a_ps_that_fails_in_the_wait_fails_the_run || true)
    git clone --no-local -q "$XFX" "$SCRATCH/mut4"; git -C "$SCRATCH/mut4" checkout -q -b mut4 2e85d15b
    SCRATCH=$SCRATCH MUT_TMP=$GATE_TMP/mut4 $LOCK "$(dirname "$0")/mutants4.sh" ;;
  r5)
    # Round 5: the pair with 80486812's harness, eight units a leg, the new
    # tests at 05d80375 (RED), the mutants. The gate is gates.sh on a clone at 80486812.
    git clone --no-local -q "$XFX" "$SCRATCH/h5"; git -C "$SCRATCH/h5" checkout -q -b h5 80486812
    H=$SCRATCH/h5
    $LOCK bash -c "
      $(declare -f sweep); FX=$FX SHA=$SHA
      sweep $H $SCRATCH/scen-rep8 'workspace-index-*' $SCRATCH/xbin/xfx-80486812 $TMP24 $SCRATCH/sw/r5-80486812-24
      sweep $H $SCRATCH/scen-rep8 'workspace-index-*' $SCRATCH/xbin/xfx-80486812 $TMP49 $SCRATCH/sw/r5-80486812-49"
    git clone --no-local -q "$XFX" "$SCRATCH/red5"; git -C "$SCRATCH/red5" checkout -q -b red5 05d80375
    git -C "$XFX" show 80486812:scripts/fxcap/test_capture.py > "$SCRATCH/red5/scripts/fxcap/test_capture.py"
    (cd "$SCRATCH/red5" && TMPDIR=$GATE_TMP python3 -I scripts/fxcap/test_capture.py \
       RunDirectory.test_a_record_copy_the_binary_deletes_changes_nothing \
       RunDirectory.test_a_record_copy_the_binary_writes_gone_changes_nothing \
       RunDirectory.test_a_record_copy_the_binary_writes_without_a_start_changes_nothing \
       RunDirectory.test_a_ps_that_fails_at_launch_fails_the_run \
       RunDirectory.test_a_relaunch_that_fails_after_respawning_says_the_pane_was_being_remade || true)
    git clone --no-local -q "$XFX" "$SCRATCH/mut5"; git -C "$SCRATCH/mut5" checkout -q -b mut5 80486812
    SCRATCH=$SCRATCH MUT_TMP=$GATE_TMP/mut5 $LOCK "$(dirname "$0")/mutants5.sh" ;;
  r6)
    # Round 6: the pairs and two real relaunch scenarios with 495fbb40's
    # harness, the new tests at f8600318 (RED), the mutants. The gate is
    # gates.sh on a clone at 495fbb40.
    git clone --no-local -q "$XFX" "$SCRATCH/h6"; git -C "$SCRATCH/h6" checkout -q -b h6 495fbb40
    H=$SCRATCH/h6
    $LOCK bash -c "
      $(declare -f sweep); FX=$FX SHA=$SHA
      sweep $H $SCRATCH/scen-rep8 'workspace-index-*' $SCRATCH/xbin/xfx-495fbb40 $TMP24 $SCRATCH/sw/r6-495fbb40-24
      sweep $H $SCRATCH/scen-rep8 'workspace-index-*' $SCRATCH/xbin/xfx-495fbb40 $TMP49 $SCRATCH/sw/r6-495fbb40-49
      sweep $H $H/scripts/fxcap/scenarios 'resume-launch,rename' $SCRATCH/xbin/xfx-495fbb40 $TMP24 $SCRATCH/sw/r6-relaunch-495fbb40-24"
    git clone --no-local -q "$XFX" "$SCRATCH/red6"; git -C "$SCRATCH/red6" checkout -q -b red6 f8600318
    git -C "$XFX" show 495fbb40:scripts/fxcap/test_capture.py > "$SCRATCH/red6/scripts/fxcap/test_capture.py"
    (cd "$SCRATCH/red6" && TMPDIR=$GATE_TMP python3 -I scripts/fxcap/test_capture.py \
       RunDirectory.test_a_relaunched_binary_whose_last_one_stays_fails_the_run \
       RunDirectory.test_a_relaunched_binary_is_the_one_waited_for \
       RunDirectory.test_a_respawn_that_fails_leaves_cleanup_the_first_binary_to_wait_for \
       RunDirectory.test_an_exit_after_the_respawn_leaves_cleanup_the_first_binary_to_wait_for \
       RunDirectory.test_a_record_copy_the_binary_made_a_fifo_never_blocks_the_run || true)
    git clone --no-local -q "$XFX" "$SCRATCH/mut6"; git -C "$SCRATCH/mut6" checkout -q -b mut6 495fbb40
    SCRATCH=$SCRATCH MUT_TMP=$GATE_TMP/mut6 $LOCK "$(dirname "$0")/mutants6.sh" ;;
  *) echo "no step $1" >&2; exit 2 ;;
esac
