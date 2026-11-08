#!/bin/bash
# usage: gate.sh <worktree> <outdir> -- every CI step, the heavy ones under the suite lock.
set -u
wt=$1 out=$2
mkdir -p "$out"
cd "$wt" || exit 2
export TMPDIR=/private/tmp/claude-501/flake-mcpmode/gate
rm -rf "$TMPDIR"; mkdir -p "$TMPDIR"
L=/private/tmp/claude-501/bin/suite-lock.sh
step() {
  local name=$1; shift
  local t0=$SECONDS
  "$@" > "$out/gate-$name.log" 2>&1
  local rc=$?
  printf '%-30s rc=%d %4ds  %s\n' "$name" "$rc" $((SECONDS - t0)) "$*" | tee -a "$out/gate.txt"
}
: > "$out/gate.txt"
echo "head $(git rev-parse HEAD)" | tee -a "$out/gate.txt"
step fmt cargo fmt --check
step clippy cargo clippy --locked --all-targets -- -D warnings
step clippy-fault cargo clippy --locked --all-targets --features fault-injection -- -D warnings
step test $L cargo test --locked --no-fail-fast
step test-fault $L cargo test --locked --features fault-injection --no-fail-fast
for s in scripts/check-*.sh scripts/test-*.sh; do step "$(basename "$s" .sh)" "./$s"; done
step fxcap-test-sweep ./scripts/fxcap/test-sweep.sh
step fxcap-test-capture $L ./scripts/fxcap/test-capture.sh
step build-release cargo build --locked --release
step build-faulty cargo build --locked --release --features fault-injection --target-dir target/faulty
step smoke ./scripts/smoke.sh target/release/xfx "$TMPDIR/xfx-smoke"
step smoke-tui $L ./scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx "$TMPDIR/xfx-smoke-tui"
echo "end $(date -u +%FT%TZ)" | tee -a "$out/gate.txt"
