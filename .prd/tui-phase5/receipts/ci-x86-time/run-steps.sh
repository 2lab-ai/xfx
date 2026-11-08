#!/bin/bash
# usage: run-steps.sh <stepdir> <worktree>
# Runs each NN.sh the way a GitHub run step does (bash -e), in order, from the
# worktree, and prints each step's wall time. Stops at the first failure.
set -u
dir=$1
cd "$2" || exit 2
for f in "$dir"/[0-9][0-9].sh; do
  name="$(sed -n 1p "$f" | sed 's/^# //')"
  start=$(date +%s)
  printf '=== STEP %s: %s (start %s)\n' "$(basename "$f" .sh)" "$name" "$(date -u +%H:%M:%SZ)"
  bash -e "$f"
  status=$?
  end=$(date +%s)
  printf '=== STEP %s: status %d, %ds\n' "$(basename "$f" .sh)" "$status" "$((end - start))"
  if [ "$status" -ne 0 ]; then
    printf '=== GATE FAILED at %s\n' "$name"
    exit "$status"
  fi
done
printf '=== GATE OK\n'
