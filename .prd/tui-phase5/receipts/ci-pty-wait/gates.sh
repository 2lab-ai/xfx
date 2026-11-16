#!/bin/bash
# The common gate, run once in the worktree under the suite lock:
#   suite-lock.sh gates.sh <worktree> <out-file>
# CARGO_TARGET_DIR and TMPDIR come from the caller. One line per step:
# "<step>: exit <code>", then each cargo test's "test result" lines and its
# failing tests. Prints no path.
set -u
cd "$1" || exit 2
out=$2
: >"$out"
step() {
	local name=$1
	shift
	"$@" >"$out.$name.log" 2>&1
	local code=$?
	echo "$name: exit $code" >>"$out"
	grep -E "^test result:|^    [a-z_0-9:]+$|panicked at" "$out.$name.log" | sed 's/^/    /' >>"$out" || true
}
step fmt cargo fmt --check
step clippy cargo clippy --locked --all-targets -- -D warnings
step clippy-fault cargo clippy --locked --all-targets --features fault-injection -- -D warnings
step test cargo test --locked --no-fail-fast
step test-fault cargo test --locked --features fault-injection --no-fail-fast
step identity scripts/check-xfx-identity.sh
step no-stubs scripts/check-no-stubs.sh
step no-secrets scripts/check-no-secrets.sh
step no-local-paths scripts/check-no-local-paths.sh
