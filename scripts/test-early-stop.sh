#!/usr/bin/env bash
#
# Proves that test-gate-env.sh and smoke.sh fail when they stop early.
#
#   scripts/test-early-stop.sh
#
# Under bash 3.2 a script stopped by an error of its own (an unset variable
# under `set -u`) leaves through its EXIT trap with the trap's status. A trap
# whose last command succeeds makes that 0, so a gate that stopped before it
# looked at anything read as a pass. And a trap on HUP, INT or TERM ran its
# cleanup and went on. Each gate script with an EXIT trap now fails any exit
# before its last line, and keeps the status bash gives it.
#
# Each case here stops a script right after it installs its EXIT trap, by a
# BASH_ENV file from scripts/early-stop-env.sh: by an unset variable under
# its `set -u`, `exit 7`, a `( exit 5 )` that `set -e` carries out, SIGTERM,
# SIGHUP or SIGINT sent to it alone, or SIGINT sent to it and a child it
# waits for. It requires the status bash gives that stop, the stop seen in
# the output with the script's EXIT trap installed, nothing past it, and the
# script's TMPDIR empty after it.
#
# The other gate scripts with an EXIT trap have these cases in their own
# tests: check-no-secrets.sh in test-check-no-secrets.sh,
# check-preview-contract.sh in test-preview-contract.sh, and
# check-xfx-identity.sh, test-check-no-secrets.sh, test-preview-contract.sh
# and this file in test-gate-env.sh.
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

# The generator itself: its `trap` stops on the EXIT trap, not on the first
# trap a script sets. A script that traps PIPE first, then EXIT, must stop
# with that EXIT trap installed and run it. (Its status is bash's: this
# script's EXIT trap has no guard, so bash 3.2 gives 0 and bash 5 gives 1.)
cat >"$work/pipe-first.sh" <<'EOF'
set -euo pipefail
trap '' PIPE
trap 'printf "the EXIT trap ran\n" >&2' EXIT
printf 'past the EXIT trap\n' >&2
EOF
./scripts/early-stop-env.sh unset >"$work/pipe-first.env"
status=0
env BASH_ENV="$work/pipe-first.env" bash "$work/pipe-first.sh" </dev/null >"$work/pipe-first.log" 2>&1 || status=$?
# shellcheck disable=SC2016 # the trap's own text
if ! grep -q -x -F -e "early stop: trap -- 'printf \"the EXIT trap ran\\n\" >&2' EXIT" "$work/pipe-first.log" ||
	! grep -q -x -F -e 'the EXIT trap ran' "$work/pipe-first.log" ||
	grep -q -F -e 'past the EXIT trap' "$work/pipe-first.log"; then
	printf 'test-early-stop: generator-exit-only: a script that traps PIPE first did not stop at its EXIT trap (exit %d):\n' "$status" >&2
	sed 's/^/  /' "$work/pipe-first.log" | cut -c 1-200 >&2
	failures=$((failures + 1))
else
	printf 'test-early-stop: ok %-32s exit %d\n' generator-exit-only "$status"
	passed=$((passed + 1))
fi

# smoke.sh takes a binary, which it never runs before its trap.
printf '#!/bin/sh\nexit 0\n' >"$work/xfx"
chmod 755 "$work/xfx"

# <script>|<the line that says it finished>
for script in "test-gate-env.sh|^test-gate-env: ok" "smoke.sh|^[0-9]+ check\(s\), 0 failure\(s\)$"; do
	done_re="${script#*|}"
	script="${script%%|*}"
	for row in unset:1 exit7:7 subshell:5 term:143 hup:129 int:130 int-group:130; do
		stop="${row%%:*}"
		want="${row#*:}"
		name="${script%.sh}-stopped-$stop"
		tmp="$work/tmp-$name"
		mkdir "$tmp"
		./scripts/early-stop-env.sh "$stop" >"$work/$name.env"
		args=()
		if [ "$script" = smoke.sh ]; then
			args=("$work/xfx" "$work/smoke-$stop")
		fi
		status=0
		# Its stderr dropped: bash names the command a signal ended, paths and
		# all.
		{ env BASH_ENV="$work/$name.env" TMPDIR="$tmp" "./scripts/$script" ${args[@]+"${args[@]}"} </dev/null \
			>"$work/$name.log" 2>&1 || status=$?; } 2>/dev/null
		left="$(find "$tmp" -mindepth 1 -print | wc -l | tr -d ' ')"
		if [ "$status" -ne "$want" ]; then
			printf 'test-early-stop: %s: it exited %d, expected %d; its last lines:\n' "$name" "$status" "$want" >&2
			tail -n 6 "$work/$name.log" | cut -c 1-200 >&2
			failures=$((failures + 1))
		elif ! grep -q -x -F -e "early stop: $stop" "$work/$name.log"; then
			printf "test-early-stop: %s: the stop never fired: no 'early stop: %s'; its last lines:\n" "$name" "$stop" >&2
			tail -n 6 "$work/$name.log" | cut -c 1-200 >&2
			failures=$((failures + 1))
		elif ! grep -q -x -e '^early stop: trap -- .* EXIT$' "$work/$name.log"; then
			printf 'test-early-stop: %s: no EXIT trap was installed when it stopped; its last lines:\n' "$name" >&2
			tail -n 6 "$work/$name.log" | cut -c 1-200 >&2
			failures=$((failures + 1))
		elif grep -q -E -e "$done_re" -e '^early stop: still running' "$work/$name.log"; then
			printf 'test-early-stop: %s: it ran on past the stop; its last lines:\n' "$name" >&2
			tail -n 6 "$work/$name.log" | cut -c 1-200 >&2
			failures=$((failures + 1))
		elif [ "$left" -ne 0 ]; then
			printf 'test-early-stop: %s: it left %d file(s) in its TMPDIR\n' "$name" "$left" >&2
			failures=$((failures + 1))
		else
			printf 'test-early-stop: ok %-32s exit %d\n' "$name" "$status"
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
