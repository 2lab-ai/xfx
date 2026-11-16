#!/usr/bin/env bash
# Train 16: train 15's gates.sh with TEST-ENV-ISOLATION's two CI steps added
# where ci.yml runs them, after the lints: `check-env-reads.py` (every read
# of the environment clippy lets through is listed) and its `--self-test`
# (the script-test rows). ci.yml changed in nothing else that runs between
# train 15's head 866747ff and this train's (`ci-steps.txt`); sweep.yml not
# at all. Nothing else changed.
# Train 15: train 14's gates.sh, unchanged but for this line (ci.yml and
# sweep.yml did not change between main 753b0a72 and this train's head).
# Train 14: train 13's gates.sh with one step added: ci.yml's gate job gained
# `check-at-search-cost.sh` (AT-RANK-PERF), after the output-cost check, and
# it runs there here too. Nothing else changed.
# Train 13's gate (train 12's gates.sh, unchanged but for this line: ci.yml
# and sweep.yml did not change between train 12's head and this train's):
# every CI step at the checkout's head, one after another -- trains 5's to
# 11's gates.sh with LOCAL-PATHS's two CI steps (`check-no-local-paths.sh`
# in the gate job, `test-check-no-local-paths.sh` on the script-test rows),
# each where ci.yml runs it: after check-no-secrets. The whole script runs inside one hold of the machine's
# suite lock (`suite-lock.sh gates.sh ...`), so every step runs under it and
# none overlaps another of this gate's. Each step's output goes to
# <log-dir>/<n>-<name>.log; one line per step to stdout: its exit (taken on the
# line right after the command, from `$?`, never from a substitution), its
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
step check-env-reads python3 -I scripts/check-env-reads.py
step check-env-reads-self-test python3 -I scripts/check-env-reads.py --self-test
step test cargo test --locked --no-fail-fast
step test-fault cargo test --locked --features fault-injection --no-fail-fast
step check-xfx-identity ./scripts/check-xfx-identity.sh
step check-no-stubs ./scripts/check-no-stubs.sh
step check-no-secrets ./scripts/check-no-secrets.sh
step check-no-local-paths ./scripts/check-no-local-paths.sh
step test-check-no-local-paths ./scripts/test-check-no-local-paths.sh
step check-preview-contract ./scripts/check-preview-contract.sh
step check-tui-preflight-cost ./scripts/check-tui-preflight-cost.sh
step check-at-search-cost ./scripts/check-at-search-cost.sh
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
