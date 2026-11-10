#!/usr/bin/env bash
# AT-FOLDERS' gate (AT-HEIGHT's, unchanged): every CI step the brief lists,
# at the checkout's head, one after another, the whole script inside one hold
# of the machine's suite lock (`suite-lock.sh gates.sh ...`), so no step
# overlaps another of this gate's. Each step's output goes to
# <log-dir>/<n>-<name>.log; one line per step to stdout: its exit, its
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
step fmt cargo fmt --check
step clippy cargo clippy --locked --all-targets -- -D warnings
step clippy-fault cargo clippy --locked --all-targets --features fault-injection -- -D warnings
step test cargo test --locked --no-fail-fast
step test-fault cargo test --locked --features fault-injection --no-fail-fast
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
step fxcap-test-capture ./scripts/fxcap/test-capture.sh
step build-release cargo build --locked --release
step build-faulty cargo build --locked --release --features fault-injection --target-dir target/faulty
step smoke ./scripts/smoke.sh target/release/xfx "$smoke/smoke"
step smoke-tui ./scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx "$smoke/smoke-tui"
echo "# $n steps, $failed failed; tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
exit "$failed"
