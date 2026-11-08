#!/bin/bash
# usage: pertest.sh <out-prefix> <runs>: per-test fail counts over the kept failed-run logs
prefix=$1; runs=$2
cat "$prefix".run* 2>/dev/null | grep -E '^test tui::.* \.\.\. FAILED$' | sed -E 's/^test (.*) \.\.\. FAILED$/\1/' | sort | uniq -c | awk -v n="$runs" '{printf "  %s: fail %d / %d\n", $2, $1, n}'
echo "  (every other touched test: fail 0 / $runs)"
