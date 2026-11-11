#!/usr/bin/env bash
# LOCAL-PATHS-SELFTEST-FLAKE's round-2 gate: the steps that read what round 2
# changed (scripts/check-no-local-paths.sh, its test, this folder), at the
# checkout's head -- gates.sh's step(), the other steps' commands unchanged,
# the self-test three times. The whole script runs inside one hold of
# the machine's suite lock (`suite-lock.sh gates.sh ...`). Each step's output
# goes to <log-dir>/<n>-<name>.log; one line per step to stdout: its exit, its
# seconds, the load at its start.
#
# usage: suite-lock.sh gates.sh <repo> <log-dir> <smoke-dir>
set -u
repo=$1 logs=$2 smoke=$3
mkdir -p "$logs" "$smoke"
cd "$repo" || exit 2
n=0 failed=0
step() {
  local name=$1; shift
  n=$((n + 1))
  local log
  log=$(printf '%s/%02d-%s.log' "$logs" "$n" "$name")
  local load start rc
  load=$(sysctl -n vm.loadavg | awk '{print $2}')
  start=$SECONDS
  "$@" > "$log" 2>&1
  rc=$?
  printf '%02d %-34s exit %d  %4ds  load %s\n' "$n" "$name" "$rc" "$((SECONDS - start))" "$load"
  [ "$rc" -eq 0 ] || failed=$((failed + 1))
}
echo "# head $(git rev-parse --short HEAD); tree clean: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
step check-xfx-identity ./scripts/check-xfx-identity.sh
step check-no-stubs ./scripts/check-no-stubs.sh
step check-no-secrets ./scripts/check-no-secrets.sh
step check-no-local-paths ./scripts/check-no-local-paths.sh
step test-check-no-local-paths ./scripts/test-check-no-local-paths.sh
step test-check-no-local-paths-2 ./scripts/test-check-no-local-paths.sh
step test-check-no-local-paths-3 ./scripts/test-check-no-local-paths.sh
step test-gate-env ./scripts/test-gate-env.sh
echo "# $n steps, $failed failed; tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
exit "$failed"
