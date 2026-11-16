#!/bin/bash
# One whole tui suite at a time (three test threads, as CI's arm64 row runs
# it), its three hostile-paste cases as CI has them -- one suite's three --
# beside <bg> background suites of the same binary that skip them
# (`--skip a_mebibyte_of_hostile_clipboard`): PTY tests like CI's other
# tests, loading the cores as the row's three vCPUs are loaded, with no
# second suite's mebibyte pastes on top. Each round runs one foreground
# suite per label, in the order given, each with its own background, so
# every label sees the same recipe, interleaved. No shell job control: each
# phase is one `xargs -P` over the foreground and its background.
#
#   one_suite.sh <out-dir> <rounds> <bg> <label>=<test-binary>...
#
# Run it with SUITE_LOCK_EXCLUSIVE=1 under the suite lock. Writes the
# foreground's output to <out-dir>/<label>.@suite.<round>.1.log (starve.sh's
# names, so its counters read it), the background's to
# <out-dir>/bg/<label>.<round>.<n>.log, and appends "<round> <label> @suite
# <exit> <seconds>" to <out-dir>/results.txt and "<round> <label> load <1-min
# load at the start>" to <out-dir>/load.txt. Prints no path.
set -u
if [ "${1:-}" = --run ]; then
	kind=$2 out=$3 label=$4 bin=$5 round=$6 n=$7
	start=$(date +%s)
	if [ "$kind" = fg ]; then
		"$bin" --test-threads=3 --nocapture >"$out/$label.@suite.$round.1.log" 2>&1
		code=$?
		echo "$round $label @suite $code $(($(date +%s) - start))" >>"$out/results.txt"
	else
		"$bin" --test-threads=3 --skip a_mebibyte_of_hostile_clipboard >"$out/bg/$label.$round.$n.log" 2>&1
	fi
	exit 0
fi
out=$1 rounds=$2 bg=$3
shift 3
mkdir -p "$out/bg"
for round in $(seq 1 "$rounds"); do
	for build in "$@"; do
		label=${build%%=*} bin=${build#*=}
		echo "$round $label load $(sysctl -n vm.loadavg | awk '{print $2}')" >>"$out/load.txt"
		{
			echo "--run fg $out $label $bin $round 0"
			for n in $(seq 1 "$bg"); do echo "--run bg $out $label $bin $round $n"; done
		} | xargs -P $((bg + 1)) -L 1 bash "$0"
	done
done
