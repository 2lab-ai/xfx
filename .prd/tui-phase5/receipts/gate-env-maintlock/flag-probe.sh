#!/usr/bin/env bash
# flag-probe.sh: which commit flags stop git from starting maintenance in a
# repository whose own config asks for it? Two forcings:
#   gc-hook -- decoy-setup-maintenance's: maintenance.strategy=gc, gc.auto=1,
#              two names in objects/17, a pre-auto-gc hook (counts its calls,
#              exits 1 at once, so gc never runs);
#   graph   -- maintenance.commit-graph.enabled=true with
#              maintenance.commit-graph.auto=-1 (run every time), default
#              strategy otherwise.
# Per flag set: does the commit start `maintenance run` / `gc --auto`
# (GIT_TRACE), does the hook run, and what new file is in .git/objects one
# second after the commit returned.
set -euo pipefail
root="$(mktemp -d "${TMPDIR:-/tmp}/flag-probe.XXXXXX")"
trap 'rm -rf "$root"' EXIT
sg() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false "$@"; }
mkdir -p "$root/hooks"
printf '#!/bin/sh\necho call >>"%s/calls"\nexit 1\n' "$root" >"$root/hooks/pre-auto-gc"
chmod 755 "$root/hooks/pre-auto-gc"
for forcing in gc-hook graph; do
	for flags in none both maintenance.auto=false gc.auto=0; do
		d="$root/$forcing-$flags"
		mkdir -p "$d"
		rm -f "$root/calls"
		sg -C "$d" init -q
		case "$forcing" in
		gc-hook)
			sg -C "$d" config maintenance.auto true
			sg -C "$d" config maintenance.strategy gc
			sg -C "$d" config gc.auto 1
			sg -C "$d" config core.hooksPath "$root/hooks"
			mkdir -p "$d/.git/objects/17"
			: >"$d/.git/objects/17/$(printf '%038d' 1)"
			: >"$d/.git/objects/17/$(printf '%038d' 2)"
			;;
		graph)
			sg -C "$d" config maintenance.commit-graph.enabled true
			sg -C "$d" config maintenance.commit-graph.auto -1
			;;
		esac
		printf 'decoy\n' >"$d/d.txt"
		sg -C "$d" add -- d.txt
		case "$flags" in
		none) guard=() ;;
		both) guard=(-c maintenance.auto=false -c gc.auto=0) ;;
		*) guard=(-c "$flags") ;;
		esac
		(cd "$d/.git/objects" && find . -type f | sort) >"$root/objects.before"
		GIT_TRACE="$root/trace" sg -C "$d" ${guard[@]+"${guard[@]}"} -c user.name=decoy \
			-c user.email=decoy@example.invalid -c commit.gpgsign=false commit -q -m decoy
		sleep 1
		spawned="$(grep -cE 'run_command: git (maintenance run|gc --auto)' "$root/trace" || true)"
		rm -f "$root/trace"
		hook=no
		if [ -e "$root/calls" ]; then hook="yes ($(wc -l <"$root/calls" | tr -d ' ') call(s))"; fi
		# What is there beyond the commit's own three objects.
		extra="$( (cd "$d/.git/objects" && find . -type f | sort) | comm -13 "$root/objects.before" - |
			grep -v -E '^\./[0-9a-f]{2}/[0-9a-f]{38}$' | tr '\n' ' ' || true)"
		printf '%-8s %-24s starts=%s hook-ran=%-12s new-non-object-files=[%s]\n' \
			"$forcing" "$flags" "$spawned" "$hook" "$extra"
	done
done
