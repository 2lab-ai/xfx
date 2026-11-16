#!/bin/bash
# <copies> whole tui suites of one build at once, three test threads each
# (as CI's arm64 row runs one), one build at a time: each round runs every
# label's phase in the order given. A runner runs one suite, whose three
# hostile-paste cases start together (libtest's name order); <copies> of
# them here put 3 x <copies> mebibyte pastes on this machine's 18 cores at
# once before the fix, and <copies> after it -- the same recipe for both, and
# the difference is what the fix changes. No shell job control: each phase
# is one `xargs -P`.
#
#   phases.sh <out-dir> <rounds> <copies> <label>=<test-binary>...
#
# Run it with SUITE_LOCK_EXCLUSIVE=1 under the suite lock. Writes
# <out-dir>/<label>.@suite.<round>.<copy>.log (--nocapture) and appends
# "<round> <label> @suite <exit> <seconds>" to <out-dir>/results.txt
# (starve.sh's names and format, so its counters read them), and "<round>
# <label> load <1-min load at the phase's start>" to <out-dir>/load.txt.
# Prints no path.
set -u
if [ "${1:-}" = --run ]; then
	out=$2 label=$3 bin=$4 round=$5 copy=$6
	start=$(date +%s)
	"$bin" --test-threads=3 --nocapture >"$out/$label.@suite.$round.$copy.log" 2>&1
	code=$?
	echo "$round $label @suite $code $(($(date +%s) - start))" >>"$out/results.txt"
	exit 0
fi
out=$1 rounds=$2 copies=$3
shift 3
mkdir -p "$out"
for round in $(seq 1 "$rounds"); do
	for build in "$@"; do
		label=${build%%=*} bin=${build#*=}
		echo "$round $label load $(sysctl -n vm.loadavg | awk '{print $2}')" >>"$out/load.txt"
		for copy in $(seq 1 "$copies"); do echo "--run $out $label $bin $round $copy"; done |
			xargs -P "$copies" -L 1 bash "$0"
	done
done
