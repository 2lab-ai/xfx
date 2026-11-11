#!/bin/bash
# AT-RANK-PERF's size against the brief's rule ("the source change outside
# tests ... about 300 lines"): src/tui/file_index.rs's lines added and removed
# against the base, split at the line `#[cfg(test)]` that opens its tests;
# src/tui/file_index/reference.rs, test-only, if the revision has it;
# and the gate's script and CI lines.
# usage: size.sh <base> <rev>   (run in the repo; size.txt is its output)
set -euo pipefail
base=$1 rev=$2
f=src/tui/file_index.rs
echo "# git diff --stat $base $rev -- src/ scripts/ .github/"
git diff --stat "$base" "$rev" -- src/ scripts/ .github/
tn=$(git show "$rev:$f" | grep -n '^#\[cfg(test)\]' | head -1 | cut -d: -f1)
to=$(git show "$base:$f" | grep -n '^#\[cfg(test)\]' | head -1 | cut -d: -f1)
echo "# $f: tests start at line $to ($base), $tn ($rev)"
git diff -U0 "$base" "$rev" -- "$f" | awk -v TN="$tn" -v TO="$to" '
  /^@@/ { split($2, o, ","); split($3, n, ","); old = substr(o[1], 2) + 0; new = substr(n[1], 2) + 0; next }
  /^\+\+\+|^---/ { next }
  /^\+/ { if (new < TN) { add++; l = substr($0, 2); if (l ~ /^[ \t]*\/\//) com++; else if (l ~ /^[ \t]*$/) bl++ } else tadd++; new++; next }
  /^-/ { if (old < TO) del++; else tdel++; old++; next }
  END { printf "outside the tests: +%d -%d (net %+d); of the added, %d comment and %d blank lines\n", add, del, add - del, com, bl
        printf "inside the tests: +%d -%d\n", tadd, tdel }'
if git cat-file -e "$rev:src/tui/file_index/reference.rs" 2>/dev/null; then
  echo "# src/tui/file_index/reference.rs (test-only): $(git show "$rev:src/tui/file_index/reference.rs" | wc -l | tr -d ' ') lines"
else
  echo "# src/tui/file_index/reference.rs: none at $rev (round 2 replaced it with the tests' unpruned oracle)"
fi
