#!/bin/bash
# stageC.sh <n>: the instrumented check, n times, on one fixed tree; any output differing from run 1 is kept
set -uo pipefail
S="$(cd "$(dirname "$0")" && pwd)"; t="$S/fixed/shapes"; n="$1"; bad=0
tag="$2"; mkdir -p "$S/$tag"
for i in $(seq 1 "$n"); do
	dbg="$S/$tag/run.$i"; mkdir -p "$dbg"
	(cd "$t" && HOME="${LPSF_HOME:-$HOME}" LPSF_DEBUG="$dbg" ./scripts/check-no-local-paths.sh) >"$dbg/out" 2>&1 </dev/null
	echo "exit=$?" >>"$dbg/out"
	if [ "$i" -eq 1 ]; then cp "$dbg/out" "$S/$tag/ref"; rm -rf "$dbg"; continue; fi
	if cmp -s "$dbg/out" "$S/$tag/ref"; then rm -rf "$dbg"; else bad=$((bad + 1)); echo "C-bad iter=$i keep=$dbg"; fi
done
echo "stage $tag: $bad differing of $((n - 1))"
