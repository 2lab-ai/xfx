#!/usr/bin/env bash
# LOCAL-PATHS: every commit of the branch, each scanned as a push publishes
# it -- its own tree, checked out in a throwaway clone -- by its own
# scripts/check-no-local-paths.sh (where it has one) and by <head>'s. One
# line a commit: the commit, each check's exit status and its last line's
# counts, and, where <head>'s check refuses it, how many of the lines it
# names are more than <base> held (by file and shape: lines <base> already
# held move with every edit, so line numbers are not compared) -- never a
# path.
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
# `<file> <shape>` for each line the check named, its line number dropped.
named() {
	sed -n -E 's/^(.*):[0-9]+: (.*)$/\1	\2/p; s/^(.*): \(a binary file.*$/\1	binary/p' "$1" | LC_ALL=C sort
}
# The head's check, run on the clone as it is checked out, into $work/$1.
head_check() {
	mkdir -p "$clone/scripts"
	cp "$work/check" "$clone/scripts/check-no-local-paths.sh"
	(cd "$clone" && bash ./scripts/check-no-local-paths.sh) >"$work/$1" 2>&1 </dev/null
	rc=$?
	g checkout -q --force HEAD
	g clean -q -f -- scripts/check-no-local-paths.sh
	return $rc
}
printf '%-9s %-50s %s\n' commit "its own check" "the head's check"
g checkout -q --force "$base"
head_check base
rc=$?
named "$work/base" >"$work/base.named"
printf '%-9s %-50s %s\n' "$(g rev-parse --short=8 "$base")" "(the base)" "exit $rc, $(summary "$work/base")"
for rev in $(g rev-list --reverse "$base..$head"); do
	g checkout -q --force "$rev"
	own='(none)'
	if [ -f "$clone/scripts/check-no-local-paths.sh" ]; then
		(cd "$clone" && bash ./scripts/check-no-local-paths.sh) >"$work/own" 2>&1 </dev/null
		rc=$?
		own="exit $rc, $(summary "$work/own")"
	fi
	head_check head
	rc=$?
	at_head="exit $rc, $(summary "$work/head")"
	if [ "$rc" -ne 0 ]; then
		# Lines beyond the base's: per file and shape, what this commit holds
		# past what the base held (`comm -23` on the sorted multisets).
		added=$(named "$work/head" | LC_ALL=C comm -23 - "$work/base.named" | wc -l | tr -d ' ')
		at_head="$at_head, $added beyond the base's"
	fi
	printf '%-9s %-50s %s\n' "$(g rev-parse --short=8 "$rev")" "$own" "$at_head"
done
