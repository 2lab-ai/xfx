#!/usr/bin/env bash
# Train 12, join check 1: each unit's own test_capture.py cases, by name, in
# the head's `test-capture.sh -v` run (jobs.sh named): the `def test_` lines
# each unit added to scripts/fxcap/test_capture.py against its base, and the
# one SWEEP-WALLCLOCK-F changed (`test_each_verbs_arguments`, its three
# refusals), each with the verdict unittest printed for it.
# usage: harness_names.sh <repo> <named-capture.log>
set -u
repo=$1 log=$2
for side in "SWEEP-WALLCLOCK-F 3be49c15 4bc9ab13" "LOCAL-PATHS ccced1f5 5609108f"; do
  set -- $side
  names=$(git -C "$repo" diff "$2" "$3" -- scripts/fxcap/test_capture.py | grep -E '^\+\s+def test_' | sed -E 's/^\+[[:space:]]+def (test_[A-Za-z0-9_]+).*/\1/')
  [ "$1" = SWEEP-WALLCLOCK-F ] && names="$names
test_each_verbs_arguments"
  n=0 ok=0
  echo "## $1 ($2..$3)"
  for t in $names; do
    # unittest -v prints a docstring's first line under the name, and the
    # verdict at the end of whichever line comes last
    line=$(awk -v t="$t (" 'index($0, t) == 1 { print; if (index($0, " ... ")) exit; getline; print; exit }' "$log" | tr '\n' ' ' | sed 's/ *$//')
    n=$((n + 1))
    case "$line" in *" ... ok"*) ok=$((ok + 1)) ;; esac
    echo "    ${line:-$t: NOT IN THE RUN}"
  done
  echo "# $1: $ok of $n ok"
done
