#!/usr/bin/env bash
#
# The `@` file picker's search, held to a frame: every keystroke looks the
# query up again over the whole index, on the thread that paints, so a
# keystroke's search has the event loop's 8 ms `TICK` (`src/tui/event_loop.rs`).
# The one test below searches fx's own benchmark inventory -- 100,000 entries,
# its eight queries (`benchmarks/file_index_bench.zig@481104fd`) -- at every
# prefix, and fails past 8 ms at the 95th percentile. The bar lives in the
# test; this script runs it where an elapsed time means something, as
# check-tui-preflight-cost.sh does its two: a **release** build (the test
# asserts only under optimization, and only searches once per query in the
# ordinary debug `cargo test`), and **alone** (`--test-threads=1`, an exact
# filter). It does not give the test the machine, so it prints the load
# average before and after: a failure is read against it, and re-measured
# rather than taken as a regression on a loaded runner.
#
# The harness's own count is checked afterwards: a renamed test would
# otherwise pass here by running nothing. And the measurement itself must be
# there: the test asserts only when it timed every prefix, and says so in the
# line `every keystroke: 1460 searches ...`. A build that kept debug
# assertions (`CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true`, say) searches
# each query once, asserts nothing and passes; without that line this script
# fails it, as check-tui-preflight-cost.sh fails a run with too few
# measurement lines.

set -euo pipefail

# BSD grep puts `GREP_OPTIONS` in front of every argument list; this script's
# options are its own (scripts/test-gate-env.sh holds it to that). The tree
# variable would make the test walk a directory instead of the inventory.
unset GREP_OPTIONS XFX_AT_BENCH_TREE

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

test_name=tui::file_index::tests::a_keystroke_over_a_hundred_thousand_entries_costs_less_than_a_frame

fail() {
	printf 'check-at-search-cost: %s\n' "$1" >&2
	exit 1
}

load() {
	uptime | sed 's/.*load/load/'
}

main() {
	local output status=0 before
	before="$(load)"
	output="$(cargo test --locked --release --lib -- \
		--exact --test-threads=1 --nocapture "$test_name" 2>&1)" || status=$?
	printf 'check-at-search-cost: before: %s\n' "$before"
	printf 'check-at-search-cost: after:  %s\n' "$(load)"
	# The measurements, readable as evidence whether or not the bar held.
	printf '%s\n' "$output" | grep -E 'entries: |keys .* rows p50|searches p50|slowest: |first search, cold: |8 ms frame' || true
	if [ "$status" -ne 0 ]; then
		printf '%s\n' "$output" | tail -n 20 | sed 's/^/    | /' >&2
		fail "the cargo test invocation failed (status $status); its last lines are above"
	fi
	if ! printf '%s\n' "$output" | grep -qE '^test result: ok\. 1 passed; 0 failed'; then
		fail "the filter did not run the @ search's cost test; a rename would look like this"
	fi
	# fx's eight queries have 73 prefixes, each timed 20 times: 1,460.
	if ! printf '%s\n' "$output" | grep -qE '^every keystroke: 1460 searches '; then
		fail "the @ search's cost test measured nothing: no 'every keystroke: 1460 searches' line"
	fi
	printf 'check-at-search-cost: ok\n'
}

main "$@"
