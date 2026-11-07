#!/usr/bin/env bash
#
# GATE-GITENV round 2b: row-time.sh for "before" (both steps at <before>)
# and "after" (test-gate-env.sh alone at <after>), alternated <rounds> times
# so that load from anything else on the machine falls on both.
#
#   row-time-rounds.sh <clone> <tools dir> <before> <after> <rounds>
set -uo pipefail
clone=$1 tools=$2 before=$3 after=$4 rounds=$5
i=1
while [ "$i" -le "$rounds" ]; do
	printf '\n### round %d: before\n' "$i"
	bash "$tools/row-time.sh" "$clone" "$before" test-check-no-secrets test-gate-env
	printf '\n### round %d: after\n' "$i"
	bash "$tools/row-time.sh" "$clone" "$after" test-gate-env
	i=$((i + 1))
done
