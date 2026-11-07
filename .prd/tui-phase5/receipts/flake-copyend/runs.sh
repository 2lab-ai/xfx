#!/usr/bin/env bash
# Runs one test of a lib test binary n times, one after another, and says
# how each went with the 1-minute load average when it began.
# usage: runs.sh <binary> <test path> <n>
set -uo pipefail
bin=$1 name=$2 n=$3
pass=0 fail=0
for ((i = 1; i <= n; i++)); do
  load=$(sysctl -n vm.loadavg | awk '{print $2}')
  if out=$(${PREFIX:-} "$bin" "$name" --exact --test-threads 1 2>&1); then
    pass=$((pass + 1)); echo "run $i pass load=$load"
  else
    fail=$((fail + 1)); echo "run $i FAIL load=$load: $(printf '%s\n' "$out" | grep -E "left:|right:|panicked at|assert" | head -3 | tr '\n' ' ')"
  fi
done
echo "total: $pass passed, $fail failed of $n"
