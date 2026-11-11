#!/usr/bin/env bash
# Train 14 (train 13's reruns_txt.sh, its calls this train's): reruns.txt,
# through redacted.py -- each reruns.sh call's own lines (sweep-rerun.sh's:
# the build, fx's sha256 before and after, the pass line) and each rerun's
# verdicts pair by pair (rerun_verdicts.py), then the same scenarios in the
# sweeps read as one run each.
# usage: reruns_txt.sh <scratch> <batch>:<scenario>[,<scenario>...]...
#        (its reruns-<batch>.out, rr<batch>/, sw-head, ref-base)
set -u
T=$1; shift
here=$(cd "$(dirname "$0")" && pwd)
{
  echo "# Train 14's reruns (reruns.sh, one hold of the suite lock a batch, sweep-rerun.sh 3 times a scenario on each tree's own debug build; \"main\" in a name is the base dcfa5146, main itself)"
  for spec in "$@"; do
    b=${spec%%:*} list=${spec#*:}
    echo; echo "## batch $b: ${list//,/, }, head and base"
    cat "$T/reruns-$b.out"
    names=() sweeps=()
    for s in ${list//,/ }; do
      names+=("head-$s" "main-$s")
      sweeps+=("sweep-head-$s=$T/sw-head" "sweep-base-$s=$T/ref-base")
    done
    python3 -I -B "$here/rerun_verdicts.py" "$T/rr$b" "${names[@]}"
    echo "# and in the two main sweeps, read as one run each"
    python3 -I -B "$here/rerun_verdicts.py" "$T" "${sweeps[@]}"
  done
} | python3 -I -B "$here/redacted.py" "$here/reruns.txt" "$T=<scratch>"
