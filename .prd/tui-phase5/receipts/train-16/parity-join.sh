#!/usr/bin/env bash
# Train 16 (new; train 15's parity-r2.txt, made by a script): the join
# commit's and the re-points' docs/parity.md edits, word by word -- each
# commit's `git diff --word-diff=plain` of the file, the words out and in,
# one changed run a line, then the rows each commit changed (by first cell)
# and, for the join commit, each edited sentence as it stands after it.
# usage: parity-join.sh <repo>
set -u
repo=$1
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
git -C "$repo" show 622b2865:docs/parity.md | python3 -I -B -c '
import sys
text = sys.stdin.read()
for start in ("fx\x27s composer also completes a typed argument from its menu before Enter, and so does",
              "**The draft\x27s keys are fx v0.0.16\x27s**",
              "behind the meta prefix too (`ESC ESC [106;5u`",
              "The meta-prefixed ones are C-j too:",
              "refuses it as not an xfx command when it takes none."):
    i = text.find(start)
    end = text.find("**", i + len(start) + 400) if i >= 0 else -1
    print(f"- {text[i:i + 1100] if i >= 0 else 'NOT FOUND: ' + start}")
    print()
'
