#!/usr/bin/env bash
# Train 14: train 13's test_counts.sh, copied; what changed for this train, if anything, is said below this line.
# Train 7's count (trains 8's to 12's test_counts.sh, unchanged but for this line), per commit: lines holding #[test] under src and tests
# (git grep -c '#\[test\]'), and '    def test_' under scripts/fxcap.
# usage: test_counts.sh <repo> <commit>...
set -u
repo=$1; shift
echo "# lines holding #[test] under src and tests (git grep -c '#\[test\]'), and '    def test_' under scripts/fxcap, per commit"
for c in "$@"; do
  r=$(git -C "$repo" grep -c '#\[test\]' "$c" -- src tests | awk -F: '{s+=$NF} END{print s}')
  f=$(git -C "$repo" grep -c '    def test_' "$c" -- scripts/fxcap | awk -F: '{s+=$NF} END{print s}')
  printf '%s %-60s rust %5d  fxcap %d\n' "$(git -C "$repo" rev-parse --short=8 "$c")" "$(git -C "$repo" log -1 --format=%s "$c" | cut -c1-60)" "$r" "$f"
done
