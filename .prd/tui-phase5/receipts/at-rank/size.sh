#!/bin/bash
# AT-RANK's size against the brief's rule ("the source change outside tests
# ... about 300 lines"): src/ lines added and removed against the base,
# split at the line `#[cfg(test)]` that opens src/tui/file_index.rs's tests
# (the only file changed), and the ranking section's length on each side.
# usage: size.sh <base> <rev>   (run in the repo; size.txt is its output for cb7c9c18 and the receipt's head)
set -euo pipefail
base=$1 rev=$2
f=src/tui/file_index.rs
echo "# git diff --stat $base $rev -- src/"
git diff --stat "$base" "$rev" -- src/
for side in "$base" "$rev"; do
  t=$(git show "$side:$f" | grep -n '^#\[cfg(test)\]' | head -1 | cut -d: -f1)
  a=$(git show "$side:$f" | grep -n '^// ranking$' | cut -d: -f1)
  b=$(git show "$side:$f" | grep -n '^// the worker$' | cut -d: -f1)
  echo "# $side: tests start at line $t; ranking section lines $a-$b = $((b - a)) lines"
done
tn=$(git show "$rev:$f" | grep -n '^#\[cfg(test)\]' | head -1 | cut -d: -f1)
to=$(git show "$base:$f" | grep -n '^#\[cfg(test)\]' | head -1 | cut -d: -f1)
git diff -U0 "$base" "$rev" -- "$f" | awk -v TN="$tn" -v TO="$to" '
  /^@@/ { split($2, o, ","); split($3, n, ","); old = substr(o[1], 2) + 0; new = substr(n[1], 2) + 0; next }
  /^\+\+\+|^---/ { next }
  /^\+/ { if (new < TN) { add++; l = substr($0, 2); if (l ~ /^[ \t]*\/\//) com++; else if (l ~ /^[ \t]*$/) bl++ } else tadd++; new++; next }
  /^-/ { if (old < TO) del++; else tdel++; old++; next }
  END { printf "outside the tests: +%d -%d (net %+d); of the added, %d comment and %d blank lines\n", add, del, add - del, com, bl
        printf "inside the tests: +%d -%d\n", tadd, tdel }'
