#!/usr/bin/env bash
#
# Proves a release build ignores the pinned usage clock.
#
# `XFX_E2E_USAGE_CLOCK_FILE` names a file that pins `/usage`'s wall clock, the
# test-only seam the capture harness's `clock` verb drives (src/usage/clock.rs).
# Only a build with debug assertions compiles its reader. Whether that holds is
# a property of the build profile, so it is checked by building each profile
# and running the one test that knows both halves: under `cargo test` it must
# print `honoured (debug assertions)`, and under `cargo test --release` --
# default features and `fault-injection`, the two release builds CI makes --
# `ignored (release)`. The release half reuses the build
# `check-tui-preflight-cost.sh` makes (`cargo test --release --lib`, the same
# features), so it costs CI one test run, not a build.
#
# A filter that matched nothing would pass `cargo test` silently, so each run
# must also report exactly one test passed.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

test_name=usage::clock::tests::the_pinned_clock_is_read_only_with_debug_assertions
failures=0

fail() {
	printf 'check-usage-clock: %s\n' "$1" >&2
	failures=$((failures + 1))
}

# run <label> <marker> <cargo test arguments>...
run() {
	local label="$1" marker="$2"
	shift 2
	local output status=0
	output="$(cargo test --locked --lib "$@" -- --exact --nocapture "$test_name" 2>&1)" || status=$?
	if [ "$status" -ne 0 ]; then
		fail "cargo test failed ($label, status $status); its last lines follow"
		printf '%s\n' "$output" | tail -n 20 | sed 's/^/    | /' >&2
		return
	fi
	if ! printf '%s\n' "$output" | grep -qE '^test result: ok\. 1 passed; 0 failed'; then
		fail "the filter did not run the one clock test ($label); a rename would look like this"
		return
	fi
	if ! printf '%s\n' "$output" | grep -qxF "usage clock: $marker"; then
		fail "the clock test did not say \`$marker\` ($label)"
		return
	fi
	printf 'check-usage-clock: %s: %s\n' "$label" "$marker"
}

main() {
	run "debug" "honoured (debug assertions)"
	run "release" "ignored (release)" --release
	run "release, fault injection" "ignored (release)" --release --features fault-injection
	if [ "$failures" -ne 0 ]; then
		printf 'check-usage-clock: %d problem(s) found\n' "$failures" >&2
		exit 1
	fi
	printf 'check-usage-clock: ok\n'
}

main "$@"
