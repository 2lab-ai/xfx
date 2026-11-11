#!/bin/bash
# Mutants of WORKSPACE-INDEX-AT-PLAN (round 5, with round 4's six) on a throwaway clone at 80486812.
# Each: one edit, the targeted tests, the clone restored. KILLED = a test fails.
set -u
M=${SCRATCH:?}/mut5
export TMPDIR=${MUT_TMP:?}
mkdir -p "$TMPDIR"
cd "$M" || exit 2
[ "$(git rev-parse --short HEAD)" = 80486812 ] || { echo "clone not at 80486812"; exit 2; }
CT="scripts/fxcap/test_capture.py RunDirectory"
ST="scripts/fxcap/test_sweep.py Sweep.test_a_units_two_runs_share_one_run_directory_named_by_digits Sweep.test_a_units_second_run_finds_nothing_its_first_left"
mutant() { # id file sed-expr test...
  local id=$1 file=$2 expr=$3; shift 3
  git checkout -q -- scripts
  sed -i '' "$expr" "$file"
  if git diff --quiet -- scripts; then echo "$id: NOT APPLIED"; return; fi
  local diffline; diffline=$(git diff -U0 -- scripts | grep -E '^[-+][^-+]' | tr '\n' ' ' | cut -c1-220)
  local killed=0 t
  for t in "$@"; do
    if ! timeout 400 python3 -I $t > "$TMPDIR/$id.log" 2>&1; then killed=1; break; fi
  done
  if [ $killed = 1 ]; then
    echo "$id: KILLED by ${t%% *} ($(grep -m1 -E 'AssertionError|Error' "$TMPDIR/$id.log" | cut -c1-120))"
  else
    echo "$id: SURVIVED"
  fi
  echo "    diff: $diffline"
  echo "    failing: $(grep -E '^(FAIL|ERROR):' "$TMPDIR/$id.log" | sed -e 's/ (__main__[^)]*)//' -e 's/^[A-Z]*: //' | sort -u | tr '\n' ' ')"
}
C=scripts/fxcap/capture.sh P=scripts/fxcap/sweep.py
ST="$ST Sweep.test_a_run_that_failed_is_a_failed_unit_and_its_path_is_not_reused"
# Round 5's.
mutant V1-cleanup-reads-the-file $C '588s/$/; { read -r pane_kind; read -r pane_pid; read -r pane_start; } < "$pane_rec" || true/' "$CT"
mutant V2-empty-kind-after-launch $C "545s/pane_set unknown/pane_set ''/" "$CT"
mutant V3-no-reset-at-relaunch $C '1321s/.*/        :/' "$CT"
# Round 4's six, at their new lines.
mutant U1-unknown-as-gone-at-launch $C "545s/pane_set unknown/pane_set gone/" "$CT"
mutant U2-unknown-as-gone-in-the-wait $C '576s/then echo unknown$/then echo gone/' "$CT"
mutant U3-unknown-record-ignored $C '594s/return 1 ;;/return 0 ;;/' "$CT"
mutant U4-no-kill-0 $C '575s/.*/  :/' "$CT"
mutant R1-no-wait $C '616s/.*/    true/' "$CT"
mutant R2-no-fail $C '616s/.*/    await_pane || true/' "$CT"
git checkout -q -- scripts
git status --short
