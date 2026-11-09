#!/usr/bin/env bash
# The gate's test counts from its step logs (gates.txt keeps one line per
# step): every 'Ran N tests' and its verdict line of the two fxcap steps, and
# each test binary's 'test result' line of the two cargo test steps, the
# binary named by its source (the target path after it dropped).
# usage: gate_counts.sh <log-dir>
set -u
logs=$1
echo "# the gate's test counts, from its step logs"
for f in "$logs"/*-fxcap-test-sweep.log "$logs"/*-fxcap-test-capture.log; do
  echo "## $(basename "$f" .log)"
  grep -E '^(Ran [0-9]+ tests|OK|FAILED)' "$f"
done
for f in "$logs"/*-test.log "$logs"/*-test-fault.log; do
  echo "## $(basename "$f" .log)"
  grep -E '^ +Running |^test result:' "$f" | sed -E 's/^( +Running [^ ]+ [^ ]+) \(.*\)$/\1/'
done
