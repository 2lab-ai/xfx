#!/usr/bin/env bash
#
# Prints the <local-path> slot held by <pid>
# (default: this script's caller's parent), read from the lock's own files,
# or says that none is. For a log to show the run it records held a slot.
#
#   lock-line.sh [pid]
pid=${1:-$PPID}
for d in <local-path>; do
	[ -f "$d/pid" ] || continue
	if [ "$(cat "$d/pid")" = "$pid" ]; then
		printf 'suite-lock: %s held by pid %s since %s for: %s\n' "${d##*/}" "$pid" "$(cat "$d/since")" "$(cat "$d/what")"
		exit 0
	fi
done
printf 'suite-lock: no slot is held by pid %s\n' "$pid"
exit 1
