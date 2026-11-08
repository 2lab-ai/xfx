#!/usr/bin/env bash
#
# GATE-GITENV round 2d: the gates that failed in gates-full.sh, run again one
# at a time in <checkout>, each with its failing tests (or all of its
# output's summary) and its exit status and time, and the load average
# before each.
#
#   gates-rerun.sh <checkout> <label>
set -uo pipefail
cd "$1" || exit 1
label=$2
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done
unset GREP_OPTIONS
printf '== %s: head %s (%s)\n' "$label" "$(git rev-parse --short HEAD)" "$(git log -1 --format=%s | cut -c 1-60)"
uname -sm
log="$(mktemp "${TMPDIR:-/tmp}/xfx-gates-rerun.XXXXXX")"
trap 'rm -f "$log"' EXIT
gate() {
	local start status=0
	printf '\n== %s\n' "$*"
	printf 'load before: %s\n' "$(uptime | sed 's/.*load averages*: //')"
	start=$(date +%s)
	"$@" </dev/null >"$log" 2>&1 || status=$?
	grep -E '^(test result:|failures:|    [a-z_:]+$|FAIL:|ERROR:|Ran |OK|FAILED|thread .* panicked)' "$log" | cut -c 1-200 | head -n 60
	printf -- '-> exit %d, %d s\n' "$status" $(($(date +%s) - start))
}
gate cargo test --locked --features fault-injection --test tui
gate cargo test --locked --lib --features fault-injection
gate ./scripts/fxcap/test-capture.sh
