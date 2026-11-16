#!/bin/bash
# The starvation recipe: PTY tests run many at once, each copy of each test of
# each build in the same round, so the builds compared share one machine
# state. A fixed number of rounds; no busy loops -- the load is the tests'
# own (a debug xfx on a pty, the harness's reader and writer, the fake
# gateway), about twice as many at once as the machine has cores, beside
# whatever else holds the machine's suite-lock slots.
#
#   starve.sh <out-dir> <rounds> <copies> <label>=<test-binary>... -- <test-name>...
#
# Run it under the suite lock (one slot: it is one suite's worth of tests).
# No shell job control: each round is one `xargs -P` over its runs. A round
# that did not run every one of them stops the run.
#
# Writes <out-dir>/<label>.<test>.<round>.<copy>.log (the test's own output)
# and appends "<round> <label> <test> <exit> <seconds>" to
# <out-dir>/results.txt. The tests' temporary files go under $TMPDIR. Extra
# arguments for every test run: $STARVE_TEST_ARGS (e.g. --nocapture).
# STARVE_BACKGROUND=1 runs every test at background QoS (`taskpolicy -b`), so
# the machine's other work -- at normal priority -- comes first: the run-to-run
# scheduling gaps a 3-core CI runner gives a PTY test, on an 18-core machine.
# The test name `@suite` runs the whole binary instead, its tests in parallel
# as `cargo test` runs them (round 3: the paced case inside the tui suite).
set -u

if [ "${1:-}" = --run ]; then
	out=$2 label=$3 bin=$4 round=$5 copy=$6 name=$7
	start=$(date +%s)
	prefix=()
	[ "${STARVE_BACKGROUND:-0}" = 1 ] && prefix=(taskpolicy -b)
	pick=(--exact "$name" --test-threads=1)
	[ "$name" = @suite ] && pick=()
	# shellcheck disable=SC2086
	${prefix[@]+"${prefix[@]}"} "$bin" ${pick[@]+"${pick[@]}"} ${STARVE_TEST_ARGS:-} \
		>"$out/$label.$name.$round.$copy.log" 2>&1
	code=$?
	echo "$round $label $name $code $(($(date +%s) - start))" >>"$out/results.txt"
	echo x >>"$out/round-$round.done"
	exit 0
fi

out=$1 rounds=$2 copies=$3
shift 3
builds=()
while [ $# -gt 0 ] && [ "$1" != -- ]; do builds+=("$1"); shift; done
[ "${1:-}" = -- ] && shift
[ ${#builds[@]} -gt 0 ] && [ $# -gt 0 ] || { echo "usage: see the header" >&2; exit 2; }
mkdir -p "$out"
want=$((${#builds[@]} * $# * copies))
for round in $(seq 1 "$rounds"); do
	for build in "${builds[@]}"; do
		for name in "$@"; do
			for copy in $(seq 1 "$copies"); do
				printf '%s\n' "--run $out ${build%%=*} ${build#*=} $round $copy $name"
			done
		done
	done | xargs -P "$want" -L 1 "$0" || { echo "round $round: xargs failed" >&2; exit 1; }
	ran=$(wc -l <"$out/round-$round.done" 2>/dev/null || echo 0)
	if [ "$ran" -ne "$want" ]; then
		echo "round $round: $ran of $want runs ran" >&2
		exit 1
	fi
done
