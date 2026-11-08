#!/usr/bin/env bash
# Train 7's merge checks, written into this directory through redacted.py:
# the token check and its negative control (this train's four merges, and
# train 6's five again as a regression), the remerge-diffs, the reference
# maps before and after the re-point commit, the re-points against their
# code, the stale references, and the overlaps.
# usage: merge-checks.sh <repo>   (run from anywhere; no lock: git and python only)
set -u
repo=$1
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$repo=<repo>"; }
M1=04ef61b0 M2=1661d59d M3=46078553 M4=54e3196b
JOINS=85b13f84 REPOINT=c468f5e1
SIDES="train6=fdc5ac0b launch-settle-f=2f1c80e7 t-u4-f=a8c7a87d models-reask=66854557 stop-next-2=f8d625c3"
{
  echo "# token_check.py (train 6's, with train 7's numbers-only rules) over the four merges"
  python3 -I -B "$here/token_check.py" "$repo" $M1 $M2 $M3 $M4; echo "# exit $?"
  echo
  echo "# the same script over train 6's five merges (regression):"
  python3 -I -B "$here/token_check.py" "$repo" a09e39c1 7fa4cc27 d55e2317 5f601851 2ff36665 \
    | diff - <(grep -v '^#' "$here/../train-6/token-check.txt" | sed '/^$/d') > /dev/null \
    && echo "# identical to train 6's token-check.txt, its comment and blank lines aside" \
    || echo "# DIFFERENT from train 6's token-check.txt"
} | rd "$here/token-check.txt"
{
  for m in $M1 $M2 $M3 $M4; do python3 -I -B "$here/token_check.py" "$repo" --self-test $m; echo "# exit $?"; done
  echo
  echo "# train 6's merges, again (its token-self-test.txt: 8, 17, 20, 14, 5)"
  for m in a09e39c1 7fa4cc27 d55e2317 5f601851 2ff36665; do python3 -I -B "$here/token_check.py" "$repo" --self-test $m | tail -1; done
} | rd "$here/token-self-test.txt"
i=0
for m in $M1 $M2 $M3 $M4; do
  i=$((i + 1))
  git -C "$repo" show --remerge-diff --format='commit %H%nMerge: %p%n%n%B' $m | rd "$here/remerge-$i.txt"
done
python3 -I -B "$here/refs.py" "$repo" $JOINS $SIDES | rd "$here/refs.txt"
python3 -I -B "$here/refs_all.py" "$repo" $JOINS $SIDES | rd "$here/refs-all-merges.txt"
python3 -I -B "$here/refs_short.py" "$repo" $JOINS $SIDES | rd "$here/refs-short-merges.txt"
python3 -I -B "$here/refs.py" "$repo" $REPOINT $SIDES | rd "$here/refs-head.txt"
{ python3 -I -B "$here/refs_all.py" "$repo" $REPOINT $SIDES; echo; python3 -I -B "$here/refs_short.py" "$repo" $REPOINT $SIDES; } | rd "$here/refs-all.txt"
python3 -I -B "$here/repointed.py" "$repo" $REPOINT | rd "$here/repointed.txt"
python3 -I -B "$here/stale.py" "$repo" $REPOINT | rd "$here/stale.txt"
python3 -I -B "$here/overlap.py" "$repo" $M1 $M2 $M3 $M4 | rd "$here/overlap.txt"
