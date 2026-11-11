#!/usr/bin/env bash
# Train 13 (train 12's reruns_txt.sh, its one call this train's): reruns.txt,
# through redacted.py -- the reruns.sh call's own lines (sweep-rerun.sh's: the
# build, fx's sha256 before and after, the pass line) and each rerun's
# verdicts pair by pair (rerun_verdicts.py), then the same scenarios in the
# two sweeps read as one run each.
# usage: reruns_txt.sh <scratch>   (its reruns-1.out, rr1, sw-head, ref-base)
set -u
T=$1
here=$(cd "$(dirname "$0")" && pwd)
{
  echo "# Train 13's reruns (reruns.sh, one hold of the suite lock, sweep-rerun.sh 3 times a scenario on each tree's own debug build; \"main\" in a name is the base f53606d1)"
  echo; echo "## the movers: resume-paging (against HISTORY-NOTICES-COPY-FX's own sweep) and p5-input-question20 (against the base), head and base"
  cat "$T/reruns-1.out"
  python3 -I -B "$here/rerun_verdicts.py" "$T/rr1" head-resume-paging main-resume-paging head-p5-input-question20 main-p5-input-question20
  echo "# and in the two sweeps, read as one run each"
  python3 -I -B "$here/rerun_verdicts.py" "$T" "sweep-head-resume-paging=$T/sw-head" "sweep-base-resume-paging=$T/ref-base" \
    "sweep-head-p5-input-question20=$T/sw-head" "sweep-base-p5-input-question20=$T/ref-base"
} | python3 -I -B "$here/redacted.py" "$here/reruns.txt" "$T=<scratch>"
