#!/usr/bin/env bash
#
# Proves that test-gate-env.sh and smoke.sh fail when they stop early.
#
#   scripts/test-early-stop.sh
#
# Under bash 3.2 -- macOS's /bin/bash -- a script stopped by an error of its
# own (an unset variable under `set -u`) leaves through its EXIT trap with
# the trap's status. A trap whose last command succeeds makes that 0, so a
# gate that stopped before it looked at anything read as a pass. And a trap on HUP, INT or TERM ran its
# cleanup and went on. Each gate script with an EXIT trap now fails any exit
# before its last line, and keeps the status bash gives it.
#
# Each case here stops a script right after it installs its EXIT trap, by a
# `trap` from BASH_ENV that installs it and then stops: by reading a variable
# no one sets, under the script's `set -u`; by `exit 7`; or by sending the
# script SIGTERM. It requires the status bash gives that stop (1, 7, 143), the
# stop seen in the output, and nothing past it. Each file unsets BASH_ENV, so
# only the script under test reads it.
#
# The other gate scripts with an EXIT trap have these cases in their own
# tests: check-no-secrets.sh in test-check-no-secrets.sh,
# check-preview-contract.sh in test-preview-contract.sh, and
# check-xfx-identity.sh, test-check-no-secrets.sh and
# test-preview-contract.sh in test-gate-env.sh.
#
# Requires python3 (smoke.sh looks for it before its trap).

set -euo pipefail

# This file's own greps read the scripts' output with options of their own.
unset GREP_OPTIONS

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

work="$(mktemp -d "${TMPDIR:-/tmp}/xfx-early-stop.XXXXXX")"
# This file is held to the same rule it tests.
finished=0
trap 'stopped=$?; rm -rf "$work"; [ "$finished" = 1 ] || exit "$((stopped == 0 ? 1 : stopped))"' EXIT

failures=0
passed=0

# shellcheck disable=SC2016 # the file's own expansions
printf '%s\n' 'unset BASH_ENV' 'trap() {' '	builtin trap "$@"' '	: "$xfx_test_unset"' '}' >"$work/stop-unset.env"
printf '%s\n' 'unset BASH_ENV' 'trap() {' '	builtin trap "$@"' '	printf "early stop: exit 7\n" >&2' '	exit 7' \
	'}' >"$work/stop-exit7.env"
printf '%s\n' 'unset BASH_ENV' 'trap() {' '	builtin trap "$@"' '	printf "early stop: SIGTERM\n" >&2' \
	'	kill -TERM $$' '	printf "early stop: still running\n" >&2' '}' >"$work/stop-term.env"

# smoke.sh takes a binary, which it never runs before its trap.
printf '#!/bin/sh\nexit 0\n' >"$work/xfx"
chmod 755 "$work/xfx"

# <script>|<the line that says it finished>
for script in "test-gate-env.sh|^test-gate-env: ok" "smoke.sh|^[0-9]+ check\(s\), 0 failure\(s\)$"; do
	done_re="${script#*|}"
	script="${script%%|*}"
	for row in "unset|1|xfx_test_unset: unbound variable" "exit7|7|early stop: exit 7" \
		"term|143|early stop: SIGTERM"; do
		stop="${row%%|*}"
		want="${row#*|}"
		fired="${want#*|}"
		want="${want%%|*}"
		name="${script%.sh}-stopped-$stop"
		args=()
		if [ "$script" = smoke.sh ]; then
			args=("$work/xfx" "$work/smoke-$stop")
		fi
		status=0
		# Its stderr dropped: bash names the command a signal ended, paths and
		# all.
		{ env BASH_ENV="$work/stop-$stop.env" "./scripts/$script" ${args[@]+"${args[@]}"} </dev/null \
			>"$work/$name.log" 2>&1 || status=$?; } 2>/dev/null
		if [ "$status" -ne "$want" ]; then
			printf 'test-early-stop: %s: it exited %d, expected %d; its last lines:\n' "$name" "$status" "$want" >&2
			tail -n 6 "$work/$name.log" | cut -c 1-200 >&2
			failures=$((failures + 1))
		elif ! grep -q -F -e "$fired" "$work/$name.log"; then
			printf "test-early-stop: %s: the stop never fired: no '%s'; its last lines:\n" "$name" "$fired" >&2
			tail -n 6 "$work/$name.log" | cut -c 1-200 >&2
			failures=$((failures + 1))
		elif grep -q -E -e "$done_re" -e '^early stop: still running' "$work/$name.log"; then
			printf 'test-early-stop: %s: it ran on past the stop; its last lines:\n' "$name" >&2
			tail -n 6 "$work/$name.log" | cut -c 1-200 >&2
			failures=$((failures + 1))
		else
			printf 'test-early-stop: ok %-28s exit %d\n' "$name" "$status"
			passed=$((passed + 1))
		fi
	done
done

if [ "$failures" -ne 0 ]; then
	printf 'test-early-stop: %d problem(s), %d case(s) passed\n' "$failures" "$passed" >&2
	exit 1
fi
printf 'test-early-stop: ok (%d cases)\n' "$passed"
finished=1
