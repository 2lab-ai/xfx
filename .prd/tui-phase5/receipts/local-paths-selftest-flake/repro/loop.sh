#!/bin/bash
# loop.sh <n> <label> : runs the shapes harness n times, prints a tally
n="$1" label="$2"
S="$(cd "$(dirname "$0")" && pwd)"
pass=0 fail=0
for i in $(seq 1 "$n"); do
	if out="$(LPSF_REPO="$LPSF_REPO" LPSF_KEEP="$S/keep" "${LPSF_BASH:-/bin/bash}" "$S/${LPSF_HARNESS:-harness-shapes.sh}" 2>&1)"; then
		pass=$((pass + 1))
	else
		fail=$((fail + 1))
		printf '%s\n' "$out" | tail -1
	fi
done
echo "$label: $pass pass, $fail fail of $n"
