#!/usr/bin/env bash
# lock-probe.sh <runs> <mode> [extra -c ...]
# Builds a fresh decoy as test-gate-env.sh's new_decoy does, then polls for
# objects/maintenance.lock (and any other new file in .git) after the commit
# returns. Prints one line per run: seen/unseen and the poll index.
# mode: plain   -- the decoy as on origin/main
#       forced  -- the decoy's own config sets gc.auto=1 (maintenance has work)
#       guarded -- the commit carries -c maintenance.auto=false -c gc.auto=0
#       maint-only, gc-only -- the commit carries one of the two
set -euo pipefail
runs="$1"
mode="$2"
root="$(mktemp -d "${TMPDIR:-/tmp}/lock-probe.XXXXXX")"
trap 'rm -rf "$root"' EXIT
sg() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false "$@"; }
seen=0
i=0
while [ "$i" -lt "$runs" ]; do
	d="$root/decoy$i"
	mkdir -p "$d"
	sg -C "$d" init -q
	if [ "$mode" = forced ]; then
		sg -C "$d" config gc.auto 1
	fi
	printf 'decoy\n' >"$d/d.txt"
	sg -C "$d" add -- d.txt
	guard=()
	case "$mode" in
	guarded) guard=(-c maintenance.auto=false -c gc.auto=0) ;;
	maint-only) guard=(-c maintenance.auto=false) ;;
	gc-only) guard=(-c gc.auto=0) ;;
	esac
	sg -C "$d" ${guard[@]+"${guard[@]}"} -c user.name=decoy -c user.email=decoy@example.invalid \
		-c commit.gpgsign=false commit -q -m decoy
	hit=""
	p=0
	# Bounded: 400 polls, no sleep longer than 5 ms each.
	while [ "$p" -lt 400 ]; do
		if [ -e "$d/.git/objects/maintenance.lock" ]; then
			hit="$p"
			break
		fi
		p=$((p + 1))
	done
	if [ -n "$hit" ]; then
		seen=$((seen + 1))
		printf 'run %d: maintenance.lock seen after the commit returned (poll %s)\n' "$i" "$hit"
	fi
	i=$((i + 1))
done
# What the forced mode leaves once the background run is done.
sleep 2
if [ "$mode" = forced ]; then
	printf 'after 2 s, decoy0/.git/objects holds: %s\n' "$(cd "$root/decoy0/.git/objects" && find . -type f | sort | tr '\n' ' ')"
fi
printf 'mode=%s runs=%d lock-seen=%d\n' "$mode" "$runs" "$seen"
