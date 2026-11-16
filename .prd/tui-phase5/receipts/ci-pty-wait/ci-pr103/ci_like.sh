#!/bin/bash
# CI's arm64 gate row, as near as this Mac gets: one whole tui suite at a
# time with three test threads (the row has 3 vCPUs and runs the suite there
# with libtest's default, one thread per CPU), beside <hogs> busy processes
# so the suite's threads share the cores about as they share three. The
# hogs are python loops that stop at the suite's end (a stop file) or after
# an hour; no shell job control -- each round is one `xargs -P` over the
# suite and its hogs. Each round runs one suite per label, in the order
# given, so every label sees the same recipe, interleaved.
#
#   ci_like.sh <out-dir> <rounds> <hogs> <label>=<test-binary>...
#
# Run it with SUITE_LOCK_EXCLUSIVE=1 under the suite lock: it loads every
# core on purpose. Writes <out-dir>/<label>.@suite.<round>.1.log (the
# suite's output, --nocapture) and appends "<round> <label> @suite <exit>
# <seconds>" to <out-dir>/results.txt (starve.sh's format, so its counters
# read it), and "<round> <label> load <1-min load at the start>" to
# <out-dir>/load.txt. Prints no path.
set -u
if [ "${1:-}" = --hog ]; then
	exec python3 -I -c '
import os, sys, time
stop, end, n = sys.argv[1], time.time() + 3600, 0
while True:
    n += 1
    if n % 200000 == 0 and (os.path.exists(stop) or time.time() > end):
        break
' "$2"
fi
if [ "${1:-}" = --suite ]; then
	out=$2 label=$3 bin=$4 round=$5 stop=$6
	start=$(date +%s)
	"$bin" --test-threads=3 --nocapture >"$out/$label.@suite.$round.1.log" 2>&1
	code=$?
	echo "$round $label @suite $code $(($(date +%s) - start))" >>"$out/results.txt"
	: >"$stop"
	exit 0
fi
out=$1 rounds=$2 hogs=$3
shift 3
mkdir -p "$out"
for round in $(seq 1 "$rounds"); do
	for build in "$@"; do
		label=${build%%=*} bin=${build#*=}
		stop="$out/.stop.$round.$label"
		rm -f "$stop"
		echo "$round $label load $(sysctl -n vm.loadavg | awk '{print $2}')" >>"$out/load.txt"
		{
			echo "--suite $out $label $bin $round $stop"
			for i in $(seq 1 "$hogs"); do echo "--hog $stop"; done
		} | xargs -P $((hogs + 1)) -L 1 bash "$0"
		rm -f "$stop"
	done
done
