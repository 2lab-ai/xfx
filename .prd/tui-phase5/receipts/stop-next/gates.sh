#!/bin/bash
# STOP-NEXT's gate: every step of ci.yml and sweep.yml's two test scripts, one at a time, in the
# worktree, inside one suite-lock slot. usage: gates.sh <worktree> <log-dir>
# Each step's exit status is read into rc before anything else runs; step 00 is `false`, which
# must read rc=1, so the runner's own capture is checked.
set -u
W="$1"; L="$2"
cd "$W" || exit 2
export TMPDIR=/private/tmp/claude-501/stopnext
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE
mkdir -p "$L"
step() {
	local n="$1" name="$2"; shift 2
	local t0=$SECONDS
	"$@" > "$L/$n-$name.log" 2>&1
	local rc=$?
	echo "$n $name rc=$rc $((SECONDS - t0))s"
}
echo "# head $(git rev-parse HEAD), tree: $(git status --porcelain --untracked-files=no | wc -l | tr -d ' ') tracked changes"
echo "# slot taken $(date -u +%FT%TZ)"
step 00 false false
step 01 fmt cargo fmt --check
step 02 clippy cargo clippy --locked --all-targets -- -D warnings
step 03 clippy-fault cargo clippy --locked --all-targets --features fault-injection -- -D warnings
step 04 test cargo test --locked --all-targets --no-fail-fast
step 05 test-fault-tui cargo test --locked --features fault-injection --test tui
step 06 test-fault-lib cargo test --locked --lib --features fault-injection
step 07 check-no-stubs ./scripts/check-no-stubs.sh
step 08 check-no-secrets ./scripts/check-no-secrets.sh
step 09 test-check-no-secrets ./scripts/test-check-no-secrets.sh
step 10 test-gate-env ./scripts/test-gate-env.sh
step 11 check-xfx-identity ./scripts/check-xfx-identity.sh
step 12 check-preview-contract ./scripts/check-preview-contract.sh
step 13 test-preview-contract ./scripts/test-preview-contract.sh
step 14 check-tui-preflight-cost ./scripts/check-tui-preflight-cost.sh
step 15 check-usage-clock ./scripts/check-usage-clock.sh
step 16 fxcap-test-sweep ./scripts/fxcap/test-sweep.sh
step 17 fxcap-test-capture ./scripts/fxcap/test-capture.sh
step 18 build-release cargo build --locked --release
step 19 build-faulty cargo build --locked --release --features fault-injection --target-dir target/faulty
step 20 smoke ./scripts/smoke.sh target/release/xfx "$TMPDIR/smoke"
step 21 smoke-tui ./scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx "$TMPDIR/smoke-tui"
echo "# slot released $(date -u +%FT%TZ)"
echo "# tree after: $(git status --porcelain --untracked-files=no | wc -l | tr -d ' ') tracked changes"
