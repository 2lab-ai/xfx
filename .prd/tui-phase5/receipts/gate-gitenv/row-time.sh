#!/usr/bin/env bash
#
# GATE-GITENV round 2b: the per-row cost of the two CI steps this unit
# touches, one after the other on an otherwise idle machine, in a clone at
# <rev>: test-check-no-secrets.sh (a ci.yml step before 2b) and
# test-gate-env.sh (which runs test-check-no-secrets.sh again, nested).
#
#   row-time.sh <clone> <rev> [step...]     (default: both)
set -uo pipefail
clone=$1 rev=$2
shift 2
cd "$clone" || exit 1
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done
unset GREP_OPTIONS
git checkout -q "$rev" || exit 1
printf '== %s at %s\n' "$(uname -sm)" "$(git rev-parse --short HEAD)"
git --version
[ "$#" -gt 0 ] || set -- test-check-no-secrets test-gate-env
total=0
for step in "$@"; do
	start=$(date +%s); status=0
	"./scripts/$step.sh" </dev/null >"${TMPDIR:-/tmp}/row-time.log" 2>&1 || status=$?
	took=$(($(date +%s) - start))
	total=$((total + took))
	printf '%-24s exit %d, %4d s   %s\n' "$step" "$status" "$took" "$(tail -n 1 "${TMPDIR:-/tmp}/row-time.log" | cut -c 1-70)"
done
printf '%-24s         %4d s\n' total "$total"
