#!/bin/bash
# Mutants of WORKSPACE-INDEX-AT-PLAN (round 6) on a throwaway clone at 495fbb40.
# Each: one edit, the targeted tests, the clone restored. KILLED = a test fails.
set -u
M=${SCRATCH:?}/mut6
export TMPDIR=${MUT_TMP:?}
mkdir -p "$TMPDIR"
cd "$M" || exit 2
[ "$(git rev-parse --short HEAD)" = 495fbb40 ] || { echo "clone not at 495fbb40"; exit 2; }
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
# Round 6's.
mutant W1-no-wait-at-relaunch $C '1338s/.*/        :/' "$CT"
mutant W2-old-record-taken-after-the-respawn $C '1334s/.*/        :/;1337s/$/; old_kind=$pane_kind old_pid=$pane_pid old_start=$pane_start old_why=$pane_why/' "$CT"
mutant W3-cleanup-forgets-the-old-record $C '622s/.*/    :/' "$CT"
mutant W4-copy-opened-in-place $C "538s/.*/  printf '%s\\\\n' \"\$@\" > \"\$pane_rec\" 2> \/dev\/null || true/" "$CT"
git checkout -q -- scripts
git status --short
