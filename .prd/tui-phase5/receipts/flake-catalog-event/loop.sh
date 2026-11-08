#!/bin/bash
# usage: loop.sh <test-binary> <n> <out-prefix> <libtest args...>
# Runs the binary n times with the same args; keeps the output of each failed run.
set -u
bin=$1; n=$2; out=$3; shift 3
pass=0; fail=0
start=$(date -u +%FT%TZ)
for i in $(seq 1 "$n"); do
	if "$bin" "$@" > "$out.run$i" 2>&1; then
		pass=$((pass + 1)); rm -f "$out.run$i"
	else
		fail=$((fail + 1))
	fi
done
echo "start=$start end=$(date -u +%FT%TZ) runs=$n pass=$pass fail=$fail load=[$(uptime | sed 's/.*averages: //')]"
