#!/usr/bin/env bash
# LAUNCH-SETTLE's gate: train 4's gates.sh, with test-capture.sh run three
# times (the unit's purpose is that it passes every time). Every CI step the
# brief lists, at the checkout's head, one
# after another. Each heavy job (both cargo test suites, test-capture.sh, which
# drives real captures, and smoke-tui) runs through the machine's suite lock,
# which admits two heavy jobs from every worker at once (the coordinator's
# rule, 2026-11-08); SUITE_LOCK names it, and with none set the heavy jobs
# run bare. Each step's output goes to <log-dir>/<n>-<name>.log; one line per
# step to stdout: its exit, its seconds, the load at its start.
#
# usage: SUITE_LOCK=<lock script> gates.sh <repo> <log-dir> <smoke-dir>
set -u
repo=$1 logs=$2 smoke=$3
lock=${SUITE_LOCK:-env}
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
step fmt cargo fmt --check
step clippy cargo clippy --locked --all-targets -- -D warnings
step clippy-fault cargo clippy --locked --all-targets --features fault-injection -- -D warnings
step test "$lock" cargo test --locked --no-fail-fast
step test-fault "$lock" cargo test --locked --features fault-injection --no-fail-fast
step check-xfx-identity ./scripts/check-xfx-identity.sh
step check-no-stubs ./scripts/check-no-stubs.sh
step check-no-secrets ./scripts/check-no-secrets.sh
step check-preview-contract ./scripts/check-preview-contract.sh
step check-tui-preflight-cost ./scripts/check-tui-preflight-cost.sh
step check-usage-clock ./scripts/check-usage-clock.sh
step test-check-no-secrets ./scripts/test-check-no-secrets.sh
step test-gate-env ./scripts/test-gate-env.sh
step test-preview-contract ./scripts/test-preview-contract.sh
step fxcap-test-sweep ./scripts/fxcap/test-sweep.sh
step fxcap-test-capture-1 "$lock" ./scripts/fxcap/test-capture.sh
step fxcap-test-capture-2 "$lock" ./scripts/fxcap/test-capture.sh
step fxcap-test-capture-3 "$lock" ./scripts/fxcap/test-capture.sh
step build-release cargo build --locked --release
step build-faulty cargo build --locked --release --features fault-injection --target-dir target/faulty
step smoke ./scripts/smoke.sh target/release/xfx "$smoke/smoke"
step smoke-tui "$lock" ./scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx "$smoke/smoke-tui"
echo "# $n steps, $failed failed; tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
exit "$failed"
