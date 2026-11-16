#!/usr/bin/env bash
# LOCAL-PATHS-EXIT-TRAP: the two forms of the guard that meet on main, run
# side by side. PR #95's check-no-local-paths.sh and its test end their trap
# `[ "$finished" = 1 ] || exit 1`; this branch's scripts end theirs
# `|| exit "$((stopped == 0 ? 1 : stopped))"`. Each script is stopped right
# after its EXIT trap by scripts/early-stop-env.sh's files, on a throwaway
# clone, with /bin/bash.
#
#   two-forms.sh <clone> <work>
#
# One line per run: the script, the stop, the status, and whether the stop
# fired with an EXIT trap installed. No path is printed.
set -u
clone=$1 work=$2
mkdir -p "$work"
cd "$clone" || exit 2
echo "# head $(git rev-parse --short HEAD); /bin/bash $(/bin/bash -c 'echo $BASH_VERSION')"
for script in check-no-local-paths test-check-no-local-paths check-no-secrets check-xfx-identity; do
	for stop in unset exit7 subshell term; do
		scripts/early-stop-env.sh "$stop" >"$work/$script-$stop.env"
		mkdir -p "$work/tmp-$script-$stop"
		status=0
		{ env BASH_ENV="$work/$script-$stop.env" TMPDIR="$work/tmp-$script-$stop" /bin/bash "scripts/$script.sh" \
			</dev/null >"$work/$script-$stop.log" 2>&1 || status=$?; } 2>/dev/null
		fired=no
		if grep -q -x -F -e "early stop: $stop" "$work/$script-$stop.log" &&
			grep -q -x -e '^early stop: trap -- .* EXIT$' "$work/$script-$stop.log"; then
			fired=yes
		fi
		printf '%-26s %-9s exit=%-3d stopped-with-EXIT-trap=%s\n' "$script.sh" "$stop" "$status" "$fired"
	done
done
echo "# tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
