#!/usr/bin/env bash
# LOCAL-PATHS: every commit of the branch, each scanned as a push publishes
# it -- its own tree, checked out in a throwaway clone -- by its own
# scripts/check-no-local-paths.sh (where it has one) and by <head>'s. One
# line a commit: the commit, each check's exit status and its last line's
# counts, never a path.
#   commits.sh <clone> <base> <head>
set -u
clone=$1 base=$2 head=$3
g() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$clone" "$@"; }
work=$(mktemp -d "${TMPDIR:-/tmp}/lp-commits.XXXXXX")
trap 'rm -rf "$work"' EXIT
g show "$head:scripts/check-no-local-paths.sh" >"$work/check"
summary() {
	# The counts, or how many lines the check named.
	if grep -q '^check-no-local-paths: ok' "$1"; then
		sed -n -E 's/^check-no-local-paths: ok \(([0-9]+) tracked file\(s\) scanned; ([0-9]+) marked.* ([0-9]+) placeholder.*$/ok, \1 files, \2 marked, \3 placeholder/p' "$1" | tail -1
	else
		printf 'refused, %s line(s) named' "$(grep -c -E '^[^ ].*:([0-9]+: | \(a binary file)' "$1" | tr -d ' ')"
	fi
}
printf '%-9s %-50s %s\n' commit "its own check" "the head's check"
for rev in $(g rev-list --reverse "$base..$head"); do
	g checkout -q --force "$rev"
	own='(none)'
	if [ -f "$clone/scripts/check-no-local-paths.sh" ]; then
		(cd "$clone" && bash ./scripts/check-no-local-paths.sh) >"$work/own" 2>&1 </dev/null
		rc=$?
		own="exit $rc, $(summary "$work/own")"
	fi
	mkdir -p "$clone/scripts"
	cp "$work/check" "$clone/scripts/check-no-local-paths.sh"
	(cd "$clone" && bash ./scripts/check-no-local-paths.sh) >"$work/head" 2>&1 </dev/null
	rc=$?
	at_head="exit $rc, $(summary "$work/head")"
	g checkout -q --force "$rev"
	g clean -q -f -- scripts/check-no-local-paths.sh
	printf '%-9s %-50s %s\n' "$(g rev-parse --short=8 "$rev")" "$own" "$at_head"
done
