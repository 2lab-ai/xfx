#!/bin/bash
# Mutants of WORKSPACE-INDEX-AT-PLAN (round 4, with round 3's no-wait and no-fail) on a throwaway clone at 2e85d15b.
# Each: one edit, the targeted tests, the clone restored. KILLED = a test fails.
set -u
M=${SCRATCH:?}/mut4
export TMPDIR=${MUT_TMP:?}
mkdir -p "$TMPDIR"
cd "$M" || exit 2
[ "$(git rev-parse --short HEAD)" = 2e85d15b ] || { echo "clone not at 2e85d15b"; exit 2; }
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
# Round 4's.
mutant U1-unknown-as-gone-at-launch $C "533s/.*/    printf 'gone\\\\n' > \"\$pane_rec\"/" "$CT"
mutant U2-unknown-as-gone-in-the-wait $C '564s/then echo unknown$/then echo gone/' "$CT"
mutant U3-unknown-record-ignored $C '583s/return 1 ;;/return 0 ;;/' "$CT"
mutant U4-no-kill-0 $C '563s/.*/  :/' "$CT"
# Round 3's two, at their new line.
mutant R1-no-wait $C '605s/.*/    true/' "$CT"
mutant R2-no-fail $C '605s/.*/    await_pane || true/' "$CT"
git checkout -q -- scripts
git status --short
