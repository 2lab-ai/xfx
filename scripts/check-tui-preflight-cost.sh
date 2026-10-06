#!/usr/bin/env bash
#
# The TUI output check's own cost gate, run under conditions where the number
# means something.
#
# Unit A puts a decoder in front of **every** frame a released binary writes, so
# what it costs is a property the product has to keep: the painter's budget is
# 8 ms a frame at 80x24 and 32 ms at 300x200 (`.prd/tui-phase3/ssot.md`
# P3-WRAP), and the two tests below measure a preflight against exactly those
# numbers. The limits live in the tests and are not passed in from here; this
# script's whole job is to run them where an elapsed time is worth asserting.
#
# Two conditions, and both are the reason this is a script rather than a line in
# the ordinary gate:
#
#   1. **Release.** An unoptimized build measures `rustc -O0`, not the product.
#      The tests therefore assert only under optimization and merely *measure*
#      in a debug build, so the ordinary `cargo test` gate carries no wall-clock
#      assertion at all.
#   2. **One test at a time.** `cargo test` runs its tests in parallel, so an
#      elapsed time measured inside the full suite includes whatever else the
#      machine was asked to do at that moment -- which is how a 20 ms case was
#      recorded at 133 ms in a parallel run. `--test-threads=1` with an exact
#      filter removes that confound.
#
# **What that does not claim.** Running two tests alone does not give them the
# machine. A loaded builder can still perturb these numbers; what is controlled
# here is the one source of interference this repository owns, and a failure is
# a signal to re-measure deliberately rather than a proof of a regression.
#
# The filters are exact, and the harness's own count is checked afterwards: a
# renamed test would otherwise make this script pass by running nothing at all,
# which is the failure mode a performance gate is most likely to die of.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# The two tests, by their exact paths. Both are `#[cfg(test)]` in
# `src/tui/check.rs`: the first measures one frame's preflight at both screen
# sizes, the second a thousand-row append replayed row by row as it scrolls.
tests=(
	tui::check::tests::a_preflight_costs_a_small_fraction_of_one_frames_budget
	tui::check::tests::a_replayed_append_costs_a_small_fraction_of_one_frames_budget
)

failures=0

fail() {
	printf 'check-tui-preflight-cost: %s\n' "$1" >&2
	failures=$((failures + 1))
}

# The tail of a captured run, indented, so that a failure carries the tool's own
# words. Bounded rather than whole, and **unfiltered**: a `grep` for the shapes
# this script expects is what hides the shapes it does not -- a compile error, a
# toolchain that refused, a panic whose message is on a line of its own.
context() {
	printf '%s\n' "$2" | tail -n "$1" | sed 's/^/    | /' >&2
}

# Runs the two tests in one release, single-threaded invocation and proves they
# really ran. `$1` is a label for the output; the rest are extra cargo flags.
measure() {
	local label="$1"
	shift
	local output status=0

	printf 'check-tui-preflight-cost: measuring (%s)\n' "$label"
	output="$(cargo test --locked --release --lib "$@" -- \
		--exact --test-threads=1 --nocapture "${tests[@]}" 2>&1)" || status=$?

	# The measurements themselves, so a run is readable as evidence rather than
	# as a pass mark. Not anchored at the start of a line: the harness prints
	# `test <name> ... ` without a newline, so the first measurement of each
	# test arrives on the end of that line.
	printf '%s\n' "$output" | grep -oE '(preflight|replay) [0-9]+x[0-9]+[^)]*\)' || true

	if [ "$status" -ne 0 ]; then
		# **No cause is named here.** This script cannot tell a budget that was
		# missed from a build that never compiled, and saying either would be a
		# guess printed as a finding; the run's own output says which it was.
		fail "the cargo test invocation failed (status $status, $label); its last lines follow"
		context 20 "$output"
		return
	fi

	# A filter that matches nothing is a green run that measured nothing, which
	# is exactly how a performance gate rots. The count is the harness's own.
	if ! printf '%s\n' "$output" |
		grep -qE "^test result: ok\. ${#tests[@]} passed; 0 failed"; then
		fail "the filters did not run all ${#tests[@]} cost tests ($label); a rename would look like this"
		context 5 "$output"
		return
	fi

	# And both workloads must have printed: the assertions are inside the tests,
	# so a test that stopped measuring would still pass them.
	# Zero matches is an **answer**, not an error: `grep` exits non-zero when it
	# finds nothing, and under `pipefail` that used to kill the script one line
	# before the diagnostic that explains it -- so a run that measured nothing
	# failed silently, which is the one thing a diagnostic must never do.
	local measured
	measured="$(printf '%s\n' "$output" | grep -oE '(preflight|replay) [0-9]+x[0-9]+' | wc -l | tr -d ' ')" ||
		measured=0
	if [ "$measured" -lt 10 ]; then
		fail "only $measured measurement lines ($label); the workloads are 8 preflight and 2 replay"
	fi
}

main() {
	measure "default features"
	# The same tests with the fault injector compiled in, because that is the
	# other configuration the gate builds and a seam that changed the checker's
	# cost would show up here and nowhere else.
	measure "fault injection" --features fault-injection

	if [ "$failures" -ne 0 ]; then
		printf 'check-tui-preflight-cost: %d problem(s) found\n' "$failures" >&2
		exit 1
	fi

	printf 'check-tui-preflight-cost: ok\n'
}

main "$@"
