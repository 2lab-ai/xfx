#!/usr/bin/env bash
# Train 8: the gate steps merge 3 (FLAKE-MCPMODE-F: scripts/fxcap/mcphttp.ts,
# scripts/fxcap/test_capture.py and its receipt) touches, at the checkout's
# head, one after another, in one hold of the suite lock: the two fxcap
# harness steps, and the steps that read every tracked file. gates.sh's
# step(), unchanged. No Rust changed, so the cargo, build and smoke steps of
# gates.txt stand.
# usage: suite-lock.sh gates-f.sh <repo> <log-dir>
set -u
repo=$1 logs=$2
mkdir -p "$logs"
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
step fmt cargo fmt --check
step check-xfx-identity ./scripts/check-xfx-identity.sh
step check-no-stubs ./scripts/check-no-stubs.sh
step check-no-secrets ./scripts/check-no-secrets.sh
step test-check-no-secrets ./scripts/test-check-no-secrets.sh
step fxcap-test-sweep ./scripts/fxcap/test-sweep.sh
step fxcap-test-capture ./scripts/fxcap/test-capture.sh
echo "# $n steps, $failed failed; tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
exit "$failed"
