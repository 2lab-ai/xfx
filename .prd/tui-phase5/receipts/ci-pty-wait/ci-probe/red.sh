#!/bin/bash
# The three hostile pastes (the paced case and the two unpaced ones), each
# build in turn, <rounds> times under each recipe given; which build goes
# first alternates by round. One binary run per build and round: the cases
# take ONE_PASTE_AT_A_TIME, so they run one after another.
#
#   red.sh <out-dir> <rounds> <build>=<test-binary>... -- <name>=<prefix>...
#
# <prefix> is a command put in front of the binary, `none` for nothing, with
# `+` for spaces (e.g. `taskpolicy+-c+utility`). Writes
# <out-dir>/<recipe>.<build>.<round>.log, each ending in a `seconds N` line.
# Prints one line per run; prints no path.
set -u
out=$1 rounds=$2
shift 2
builds=()
while [ "$1" != -- ]; do builds+=("$1"); shift; done
shift
mkdir -p "$out"
for round in $(seq 1 "$rounds"); do
	order=("${builds[@]}")
	if [ $((round % 2)) -eq 0 ]; then
		order=()
		for ((i = ${#builds[@]} - 1; i >= 0; i--)); do order+=("${builds[i]}"); done
	fi
	for recipe in "$@"; do
		name=${recipe%%=*} prefix=${recipe#*=}
		[ "$prefix" = none ] && prefix=""
		prefix=${prefix//+/ }
		for build in "${order[@]}"; do
			label=${build%%=*} bin=${build#*=}
			log="$out/$name.$label.$round.log"
			start=$(date +%s)
			# shellcheck disable=SC2086 # the prefix is words
			$prefix "$bin" a_mebibyte_of_hostile_clipboard --nocapture >"$log" 2>&1
			code=$?
			echo "seconds $(($(date +%s) - start))" >>"$log"
			echo "$name $label round $round: exit $code, $(grep -c ' \.\.\. ok$' "$log") ok, $(grep -c ' \.\.\. FAILED$' "$log") FAILED"
		done
	done
done
