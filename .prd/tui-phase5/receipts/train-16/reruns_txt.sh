#!/usr/bin/env bash
# Train 16: train 15's reruns_txt.sh, its header this train's and a head-only
# scenario allowed (a `head-` name with no `main-` run is read alone).
# Train 15: train 14's reruns_txt.sh, its calls this train's: reruns.txt,
# through redacted.py -- rerun-job.sh's own lines (each tree's build, then
# sweep-rerun.sh's: fx's sha256 before and after, the pass line) and each
# rerun's verdicts pair by pair (rerun_verdicts.py). The sweeps' own reading
# of the same pairs is compare-base.txt's (their raw output was lost to the
# machine's /tmp cleanup, so it is not read again here as train 14 did).
# usage: reruns_txt.sh <out-dir> <scenario>...   (its reruns-1.out and rr/)
set -u
T=$1; shift
here=$(cd "$(dirname "$0")" && pwd)
{
  echo "# Train 16's reruns (reruns.sh in one hold of the suite lock, on the two sweeps' own debug builds: sweep-rerun.sh 3 times a scenario on each tree; \"main\" in a name is the base 866747ff)"
  grep -v '^exit' "$T/reruns-1.out"
  names=()
  for s in "$@"; do names+=("head-$s"); [ -e "$T/rr/main-$s-1" ] && names+=("main-$s"); done
  python3 -I -B "$here/rerun_verdicts.py" "$T/rr" "${names[@]}"
} | python3 -I -B "$here/redacted.py" "$here/reruns.txt" "$T=<rerun-dir>"
