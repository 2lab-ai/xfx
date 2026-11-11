#!/usr/bin/env bash
#
# Writes, to standard output, a BASH_ENV file that stops the script reading
# it. The early-stop cases of test-check-no-secrets.sh,
# test-preview-contract.sh, test-gate-env.sh and test-early-stop.sh all take
# their files from here, so they stop a script the same way.
#
#   scripts/early-stop-env.sh <stop> [<hook>] >"$work/stop.env"
#   BASH_ENV="$work/stop.env" <script> ...
#
# <stop>, and the status bash then gives the script:
#   unset      reads a variable no one sets, under the script's `set -u`: 1
#   exit7      runs `exit 7`: 7
#   subshell   runs `( exit 5 )`, which `set -e` carries to the script: 5
#   term, hup, int
#              sends that signal to the script alone, between commands: 143,
#              129, 130
#   int-group  sends SIGINT to the script and to a child it is waiting for,
#              as a terminal's Ctrl-C or a cancelled CI step does: 130
#   arith      divides by zero; bash drops the command that holds it and goes
#              on, so this is no stop: the script must notice what it lost
# A SIGINT sent to the script alone while it waits for a child is not here:
# bash then goes on once the child exits, by its own rule for SIGINT.
#
# <hook>, where the stop happens:
#   trap-exit (the default)  in a `trap` whose arguments name EXIT, once that
#                            trap is installed; any other `trap` only installs
#   printf=<glob>            in a `printf` whose arguments, joined by spaces,
#                            match <glob>, before it prints
#   read=<n>                 in the script's <n>-th `read`, before it reads
#
# Before it stops, the file prints `early stop: ` and the script's EXIT trap
# then (`trap -p EXIT`), and `early stop: <stop>`, to standard error; after a
# signal it prints `early stop: still running` if the script goes on. It
# unsets BASH_ENV first, so only the script it is handed to reads it, not
# that script's children.

# shellcheck disable=SC2016 # the file's own expansions, written as text
set -euo pipefail

stop="${1:-}"
hook="${2:-trap-exit}"

case "$stop" in
unset) body=': "$xfx_test_unset"' ;;
exit7) body='exit 7' ;;
subshell) body='( exit 5 )' ;;
term | hup | int)
	signal="$(printf '%s' "$stop" | tr '[:lower:]' '[:upper:]')"
	body="kill -$signal \$\$"
	;;
int-group) body="sh -c 'kill -INT \"\$1\"; kill -INT \$\$' sh \"\$\$\"" ;;
arith) body=': $((1 / 0))' ;;
*)
	printf 'early-stop-env: no stop %s\n' "'$stop'" >&2
	exit 2
	;;
esac

printf '%s\n' 'unset BASH_ENV'
printf '%s\n' 'xfx_early_stop() {'
printf '\t%s\n' 'builtin printf "early stop: %s\n" "$(builtin trap -p EXIT)" >&2'
printf '\t%s\n' "builtin printf 'early stop: %s\\n' '$stop' >&2"
printf '\t%s\n' "$body"
printf '\t%s\n' "builtin printf 'early stop: still running\\n' >&2"
printf '%s\n' '}'

case "$hook" in
trap-exit)
	printf '%s\n' 'trap() {' \
		'	builtin trap "$@" || return' \
		'	local name' \
		'	for name in "${@:2}"; do' \
		'		case "$name" in' \
		'		EXIT | 0)' \
		'			xfx_early_stop' \
		'			return' \
		'			;;' \
		'		esac' \
		'	done' \
		'}'
	;;
printf=*)
	glob="${hook#printf=}"
	# Quoted for a single-quoted word; used unquoted as a case pattern.
	printf "xfx_early_stop_glob='%s'\n" "${glob//\'/\'\\\'\'}"
	printf '%s\n' 'printf() {' \
		'	case "$*" in' \
		'	$xfx_early_stop_glob) xfx_early_stop ;;' \
		'	esac' \
		'	builtin printf "$@"' \
		'}'
	;;
read=*)
	count="${hook#read=}"
	case "$count" in
	'' | *[!0-9]*)
		printf 'early-stop-env: read=%s is not a count\n' "'$count'" >&2
		exit 2
		;;
	esac
	printf 'xfx_early_stop_reads=0\n'
	printf '%s\n' 'read() {' \
		'	xfx_early_stop_reads=$((xfx_early_stop_reads + 1))' \
		"	if [ \"\$xfx_early_stop_reads\" -eq $count ]; then" \
		'		xfx_early_stop' \
		'	fi' \
		'	builtin read "$@"' \
		'}'
	;;
*)
	printf 'early-stop-env: no hook %s\n' "'$hook'" >&2
	exit 2
	;;
esac
