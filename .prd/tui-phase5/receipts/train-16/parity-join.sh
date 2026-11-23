#!/usr/bin/env bash
# Train 16 (new; train 15's parity-r2.txt, made by a script): the join
# commit's and the re-points' docs/parity.md edits, word by word -- each
# commit's `git diff --word-diff=plain` of the file, the words out and in,
# one changed run a line, then the rows each commit changed (by first cell)
# and, for the join commit, each edited sentence as it stands after it.
# Round 2: its last section's python moved to parity_sentences.py (gpt6's NTH
# 2: the inline `-c` quoting broke it, a SyntaxError in round 1's output).
# usage: parity-join.sh <repo>
set -u
repo=$1
here=$(cd "$(dirname "$0")" && pwd)
for c in 622b2865 c637ed4c 15c9744b; do
  echo "## $c: $(git -C "$repo" log -1 --format=%s "$c" | cut -c1-90)"
  echo "# lines changed: $(git -C "$repo" diff --numstat "$c^" "$c" -- docs/parity.md)"
  git -C "$repo" diff --word-diff=plain --word-diff-regex='[^[:space:]]+' "$c^" "$c" -- docs/parity.md \
    | grep -oE '\[-[^]]*-\]|\{\+[^}]*\+\}' | cut -c1-400
  echo "# rows changed (first cell):"
  git -C "$repo" diff -U0 "$c^" "$c" -- docs/parity.md | grep '^+|' | cut -d'|' -f2 | sed 's/^/  /'
  echo
done
echo "## the join commit's sentences as they stand at 622b2865"
git -C "$repo" show 622b2865:docs/parity.md | python3 -I -B "$here/parity_sentences.py"
echo "# exit $?"
