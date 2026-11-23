#!/bin/bash
# The whole tui test binary, <runs> times under each recipe given, one run
# at a time: what else a utility clamp does to the suite, the product's own
# timers included (§13's look-ahead).
#
#   whole_suite.sh <out-dir> <test-binary> <runs> <name>=<prefix>...
#
# <prefix> as in red.sh. Writes <out-dir>/<name>.<n>.log; prints one line
# per run (the result line and every failed test); prints no path.
set -u
out=$1 bin=$2 runs=$3
shift 3
mkdir -p "$out"
for n in $(seq 1 "$runs"); do
	for recipe in "$@"; do
		name=${recipe%%=*} prefix=${recipe#*=}
		[ "$prefix" = none ] && prefix=""
		prefix=${prefix//+/ }
		log="$out/$name.$n.log"
		# shellcheck disable=SC2086 # the prefix is words
		$prefix "$bin" >"$log" 2>&1
		failed=$(grep -E '^test .* \.\.\. FAILED$' "$log" | awk '{print $2}' | sort | tr '\n' ' ')
		echo "$name run $n: $(grep -E '^test result:' "$log" | tail -1); failed: ${failed:-none}"
	done
done
