#!/usr/bin/env bash
# Train 9's merge checks, written into this directory through redacted.py:
# the token check and its negative control over the merges (train 7's
# token_check.py, byte for byte), the remerge-diffs, the reference maps
# before the receipt (at the join tests' commit) and at the head, the stale
# references, and the overlaps. Train 8's merge-checks.sh with this train's
# merges and sides; no re-point (no reference the merges moved).
# usage: merge-checks.sh <repo> <head> <merge>...   (no lock: git and python only)
set -u
repo=$1 head=$2; shift 2
merges=("$@")
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$repo=<repo>"; }
JOINS=afe31cb2
SIDES=(main=ccced1f5 stop-in-catalog-wait=05a0a857 questions-20=3824f6ba)
[ -n "${HNS:-}" ] && SIDES+=("history-notice-stop=$HNS")
{
  echo "# token_check.py (train 7's, unchanged: $(cmp -s "$here/token_check.py" "$here/../train-7/token_check.py" && echo 'byte for byte the same file' || echo 'DIFFERENT from train 7s')) over the merges"
  python3 -I -B "$here/token_check.py" "$repo" "${merges[@]}"; echo "# exit $?"
} | rd "$here/token-check.txt"
{
  for m in "${merges[@]}"; do python3 -I -B "$here/token_check.py" "$repo" --self-test "$m"; echo "# exit $?"; done
} | rd "$here/token-self-test.txt"
i=0
for m in "${merges[@]}"; do
  i=$((i + 1))
  git -C "$repo" show --remerge-diff --format='commit %H%nMerge: %p%n%n%B' "$m" | rd "$here/remerge-$i.txt"
done
python3 -I -B "$here/refs.py" "$repo" $JOINS "${SIDES[@]}" | rd "$here/refs.txt"
python3 -I -B "$here/refs_all.py" "$repo" $JOINS "${SIDES[@]}" | rd "$here/refs-all-merges.txt"
python3 -I -B "$here/refs_short.py" "$repo" $JOINS "${SIDES[@]}" | rd "$here/refs-short-merges.txt"
python3 -I -B "$here/refs.py" "$repo" "$head" "${SIDES[@]}" | rd "$here/refs-head.txt"
{ python3 -I -B "$here/refs_all.py" "$repo" "$head" "${SIDES[@]}"; echo; python3 -I -B "$here/refs_short.py" "$repo" "$head" "${SIDES[@]}"; } | rd "$here/refs-all.txt"
python3 -I -B "$here/stale.py" "$repo" "$head" | rd "$here/stale.txt"
python3 -I -B "$here/overlap.py" "$repo" "${merges[@]}" | rd "$here/overlap.txt"
