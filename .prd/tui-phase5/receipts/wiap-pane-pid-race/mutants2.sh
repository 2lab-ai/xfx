#!/bin/bash
# Mutants of WIAP-PANE-PID-RACE on a throwaway clone at fc5e4523.
# Each: one edit, LaunchWait's tests, the clone restored. KILLED = a test fails.
set -u
M=${SCRATCH:?}/mut2
export TMPDIR=${MUT_TMP:?}
mkdir -p "$TMPDIR"
cd "$M" || exit 2
[ "$(git rev-parse --short HEAD)" = fc5e4523 ] || { echo "clone not at fc5e4523"; exit 2; }
CT="scripts/fxcap/test_capture.py LaunchWait"
C=scripts/fxcap/capture.sh
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
mutant P1-reaped-read-as-unknown $C "573s/pane_set gone/pane_set unknown/" "$CT"
mutant P2-no-look-again $C '565s/i < 20/i < 1/' "$CT"
mutant P3-no-exit-read-as-gone $C '572s/\[\[ -z \$pid \&\& -n \$exit \]\]/[[ -z $pid ]]/' "$CT"
mutant P4-pid-only-asked $C "566s/'#{pane_pid}:#{pane_dead_status}#{pane_dead_signal}'/'#{pane_pid}:'/" "$CT"
mutant P5-no-record-at-relaunch $C '1364s/.*/        :/' "$CT"
git checkout -q -- scripts
git status --short
