#!/usr/bin/env bash
# Train 13 (train 12's gate_counts.sh, and the smoke steps' last lines):
# gate-counts.txt's lines, from gates.sh's step logs (gates.txt keeps each
# log's last lines only): every harness suite's 'Ran N tests' and its
# verdict, each cargo test binary's 'test result' line and their sums, the
# one-line verdicts of the local-path check, its test and test-gate-env, and
# the two smoke steps' verdict lines.
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
  grep -E '^(test-)?check-no-local-paths: |^test-gate-env: (ok \(|[0-9]+ problem)|ci-script-tests-per-os|decoy-setup-|test-check-no-local-paths' "$f" | cut -c1-220
done
for f in "$logs"/*-smoke.log "$logs"/*-smoke-tui.log; do
  echo "## $(basename "$f" .log)"
  grep -E '^smoke-tui: |^[0-9]+ check\(s\), [0-9]+ failure' "$f"
done
