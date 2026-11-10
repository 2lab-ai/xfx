#!/bin/bash
# AT-RANK-PERF's size against the brief's rule ("the source change outside
# tests ... about 300 lines"): src/tui/file_index.rs's lines added and removed
# against the base, split at the line `#[cfg(test)]` that opens its tests;
# src/tui/file_index/reference.rs, which is compiled only under `cfg(test)`;
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
echo "# src/tui/file_index.rs's '#[cfg(test)] mod reference;' (test-only): $(git show "$rev:src/tui/file_index/reference.rs" | wc -l | tr -d ' ') lines, of them (17 on) $(git show "$rev:src/tui/file_index/reference.rs" | sed -n '17,$p' | wc -l | tr -d ' ') AT-RANK's ranking section verbatim"
if diff <(git show "$base:$f" | sed -n 527,1152p) <(git show "$rev:src/tui/file_index/reference.rs" | sed -n '17,$p') > /dev/null; then
  echo "# reference.rs from line 17 = $base:$f lines 527-1152: identical"
else
  echo "# reference.rs from line 17 = $base:$f lines 527-1152: DIFFERS"
fi
