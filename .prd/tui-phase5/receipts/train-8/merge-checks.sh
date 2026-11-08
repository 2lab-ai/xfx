#!/usr/bin/env bash
# Train 8's merge checks, written into this directory through redacted.py:
# the token check and its negative control over the three merges (train 7's
# token_check.py, byte for byte), the remerge-diffs, the reference maps before
# the re-point commit (at the join test's commit) and at the head, the
# re-points against their code, the stale references, and the overlaps.
# usage: merge-checks.sh <repo> <head>   (no lock: git and python only)
set -u
repo=$1 head=$2
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$repo=<repo>"; }
M1=9a465471 M2=056b1b01 M3=c4bb4c7f
JOINS=3bf99857
SIDES="main=58c92ba2 grid-row-cost=a3f90ef5 flake-mcpmode=2971aeff flake-mcpmode-f=6a9a6a9d"
{
  echo "# token_check.py (train 7's, unchanged: $(cmp -s "$here/token_check.py" "$here/../train-7/token_check.py" && echo 'byte for byte the same file' || echo 'DIFFERENT from train 7s')) over the three merges"
  python3 -I -B "$here/token_check.py" "$repo" $M1 $M2 $M3; echo "# exit $?"
} | rd "$here/token-check.txt"
{
  for m in $M1 $M2 $M3; do python3 -I -B "$here/token_check.py" "$repo" --self-test $m; echo "# exit $?"; done
} | rd "$here/token-self-test.txt"
i=0
for m in $M1 $M2 $M3; do
  i=$((i + 1))
  git -C "$repo" show --remerge-diff --format='commit %H%nMerge: %p%n%n%B' $m | rd "$here/remerge-$i.txt"
done
# shellcheck disable=SC2086
python3 -I -B "$here/refs.py" "$repo" $JOINS $SIDES | rd "$here/refs.txt"
python3 -I -B "$here/refs_all.py" "$repo" $JOINS $SIDES | rd "$here/refs-all-merges.txt"
python3 -I -B "$here/refs_short.py" "$repo" $JOINS $SIDES | rd "$here/refs-short-merges.txt"
python3 -I -B "$here/refs.py" "$repo" $head $SIDES | rd "$here/refs-head.txt"
{ python3 -I -B "$here/refs_all.py" "$repo" $head $SIDES; echo; python3 -I -B "$here/refs_short.py" "$repo" $head $SIDES; } | rd "$here/refs-all.txt"
python3 -I -B "$here/repointed.py" "$repo" $head | rd "$here/repointed.txt"
python3 -I -B "$here/stale.py" "$repo" $head | rd "$here/stale.txt"
python3 -I -B "$here/overlap.py" "$repo" $M1 $M2 $M3 | rd "$here/overlap.txt"
