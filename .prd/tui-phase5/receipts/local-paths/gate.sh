#!/usr/bin/env bash
# LOCAL-PATHS gate: every step of CI's gate job (and the sweep workflow's
# harness tests), in the worktree at its head, one at a time. One line per
# step: exit status, seconds, the step. Logs in $logs.
#   gate.sh <tree> <logs>   (run it under the suite lock)
set -u
tree=$1 logs=$2
mkdir -p "$logs"
cd "$tree" || exit 2
# A short TMPDIR of its own under the caller's: under a long one some tui
# tests' paths pass 120 columns and a socket path passes SUN_LEN.
export TMPDIR="${TMPDIR:-/tmp}/lp-gate"
rm -rf "$TMPDIR" && mkdir -p "$TMPDIR"
printf 'head %s\n' "$(git rev-parse HEAD)"
n=0 bad=0
step() {
  local name=$1 start rc
  shift
  n=$((n + 1))
  start=$(date +%s)
  "$@" >"$logs/$(printf '%02d' $n)-$name.log" 2>&1 </dev/null
  rc=$?
  [ "$rc" -eq 0 ] || bad=$((bad + 1))
  printf '%-4s %5ss  %s\n' "$rc" "$(( $(date +%s) - start ))" "$name"
}
step fmt cargo fmt --check
step clippy cargo clippy --locked --all-targets -- -D warnings
step clippy-fi cargo clippy --locked --all-targets --features fault-injection -- -D warnings
step test cargo test --locked --all-targets --no-fail-fast
step test-fi cargo test --locked --features fault-injection --no-fail-fast
step check-no-stubs ./scripts/check-no-stubs.sh
step check-no-secrets ./scripts/check-no-secrets.sh
step check-no-local-paths ./scripts/check-no-local-paths.sh
step test-check-no-local-paths ./scripts/test-check-no-local-paths.sh
step test-gate-env ./scripts/test-gate-env.sh
step check-xfx-identity ./scripts/check-xfx-identity.sh
step check-preview-contract ./scripts/check-preview-contract.sh
step test-preview-contract ./scripts/test-preview-contract.sh
step check-tui-preflight-cost ./scripts/check-tui-preflight-cost.sh
step check-usage-clock ./scripts/check-usage-clock.sh
step build-release cargo build --locked --release
step smoke ./scripts/smoke.sh target/release/xfx "$TMPDIR/xfx-smoke"
step build-faulty cargo build --locked --release --features fault-injection --target-dir target/faulty
step smoke-tui ./scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx "$TMPDIR/xfx-smoke-tui"
step test-sweep ./scripts/fxcap/test-sweep.sh
printf 'steps %d, failed %d; tree %s\n' "$n" "$bad" "$( [ -z "$(git status --porcelain)" ] && echo clean || echo dirty)"
