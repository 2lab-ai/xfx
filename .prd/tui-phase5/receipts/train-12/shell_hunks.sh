#!/usr/bin/env bash
# Train 12, join check 4: where LOCAL-PATHS and INPUT-ROW-LIMIT each changed
# src/tui/shell.rs (their hunks against their own bases, `git diff -U0`), and
# where each base's tests module starts: a hunk past that line is test code.
# usage: shell_hunks.sh <repo>
set -u
repo=$1
for side in "LOCAL-PATHS ccced1f5 5609108f" "INPUT-ROW-LIMIT 32afd307 3d3d56c8"; do
  set -- $side
  echo "## $1 ($2..$3): its base's tests module starts at line $(git -C "$repo" show "$2:src/tui/shell.rs" | grep -n '^mod tests {' | cut -d: -f1)"
  git -C "$repo" diff -U0 "$2" "$3" -- src/tui/shell.rs | grep '^@@' | sed 's/^/    /'
  echo "## $1: the files it changed under src/"
  git -C "$repo" diff --stat "$2" "$3" -- src | sed 's/^/    /'
done
