#!/usr/bin/env bash
# Train 14 (new): main's gate counts against the head's, for the gate's
# comparison -- train 13's join of main gated its code head 0dbfaf39
# (../train-13/join-main/); main, dcfa5146, is that code (the diff outside
# .prd/ is empty), so its counts are main's. Then each test binary whose
# `cargo test` count differs (this folder's gate-counts.txt against
# join-main's, the plain run), and the test attributes the range adds and
# deletes under src/ (`git diff dcfa5146 <head>`). Output: main-gate.txt.
# usage: main-gate.sh <repo> <head>
set -u
repo=$1 head=$2
here=$(cd "$(dirname "$0")" && pwd)
binaries() { sed -n '/## 04-test/,/# sum/p' "$1" | grep -E 'Running|test result' | sed -E 's/; finished in .*//' |
  awk '/Running/ {name = ($2 == "unittests") ? $3 : $2; next} {print name ": " $0; name = "doc-tests"}'; }
{
  echo "# ../train-13/join-main/gates.txt: $(grep -m1 '^# head' "$here/../train-13/join-main/gates.txt")"
  echo "# git diff --stat 0dbfaf39 dcfa5146 outside .prd/: $(git -C "$repo" diff --stat 0dbfaf39 dcfa5146 -- . ':!.prd' | tail -1 | grep . || echo none)"
  echo "# ../train-13/join-main/gate-counts.txt's sums (cargo test, then with fault injection):"
  grep '^# sum' "$here/../train-13/join-main/gate-counts.txt"
  echo "# the binaries whose cargo test count differs, main | $head (gate-counts.txt):"
  paste -d'|' <(binaries "$here/../train-13/join-main/gate-counts.txt") <(binaries "$here/gate-counts.txt") | awk -F'|' '$1 != $2' | sed 's/^/  /'
  echo "# test attributes git diff dcfa5146 $head adds (+) and deletes (-) under src/:"
  git -C "$repo" diff dcfa5146 "$head" -- src | grep -E '^[-+][[:space:]]*#\[[a-z_:]*test\]' | sed -E 's/^([-+])[[:space:]]*/\1 /' | sort | uniq -c | sed 's/^/  /'
} > "$here/main-gate.txt"
