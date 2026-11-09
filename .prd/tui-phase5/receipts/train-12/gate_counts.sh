#!/usr/bin/env bash
# Train 12: gate-counts.txt's lines, from gates.sh's step logs (gates.txt keeps
# each log's last lines only): every harness suite's 'Ran N tests' and its
# verdict, each cargo test binary's 'test result' line and their sums, and
# the one-line verdicts of the local-path check, its test and
# test-gate-env (with its CI-coverage line for the script tests).
# usage: gate_counts.sh <log-dir>
set -u
logs=$1
echo "# The gate's counts, from its step logs."
for f in "$logs"/*fxcap-test-sweep.log "$logs"/*fxcap-test-capture.log; do
  echo "## $(basename "$f" .log)"
  grep -E '^Ran [0-9]+ tests|^OK|^FAILED' "$f"
done
for f in "$logs"/*-test.log "$logs"/*-test-fault.log; do
  echo "## $(basename "$f" .log): each binary's 'test result' line, then the sums"
  grep -E '^     Running |^test result' "$f" | sed -E 's# \(target/[^)]*\)##'
  grep -h '^test result' "$f" | awk '{p+=$4; f+=$6; i+=$8} END{print "# sum: " p " passed, " f " failed, " i " ignored"}'
done
for f in "$logs"/*check-no-local-paths.log "$logs"/*test-gate-env.log; do
  echo "## $(basename "$f" .log)"
  grep -E '^(test-)?check-no-local-paths: |^test-gate-env: (ok \(|[0-9]+ problem)|ci-script-tests-per-os|test-check-no-local-paths' "$f" | cut -c1-220
done
