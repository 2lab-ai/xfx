#!/usr/bin/env bash
# LOCAL-PATHS-EXIT-TRAP: the late stops. verdicts.sh stops each script right
# after its trap; these stop three scripts late, where a stop at the base
# exits 0 with the script's own ok line or with no line at all.
#
#   late.sh <bash> <checkout> <work> [<script>...]
#
#   check-preview-contract.sh, SIGTERM as actionlint starts (a BASH_ENV
#     `actionlint` sends it, then runs the real one; its lines go to a file of
#     their own, as the check sends actionlint's stderr to its own file): the
#     trap on TERM ran the cleanup, actionlint wrote to its already-open file,
#     and the check went on to its ok line;
#   test-check-no-secrets.sh, SIGTERM just before its ok line (a BASH_ENV
#     `printf` sends it when given that line's format);
#   check-no-secrets.sh, an unset variable read just before its ok line, once
#     its whole scan is done and every trap is installed.
# One line per run, no path in it; the logs stay in <work>.
set -u
bash_bin=$1 repo=$2 work=$3
shift 3
want=" $* "
mkdir -p "$work"
# shellcheck disable=SC2016 # the files' own expansions
{
	# The check sends actionlint's output, stderr too, to a file of its own,
	# so this function writes its lines to "$work/marks" instead.
	printf '%s\n' 'unset BASH_ENV' 'actionlint() {' "	printf 'early stop: SIGTERM\\n' >>'$work/marks'" '	kill -TERM $$' \
		"	printf 'early stop: still running\\n' >>'$work/marks'" '	command actionlint "$@"' '}' >"$work/actionlint.env"
	printf '%s\n' 'unset BASH_ENV' 'printf() {' '	case "${1:-}" in' "	'test-check-no-secrets: ok ('*)" \
		'		builtin printf "early stop: SIGTERM\n" >&2' '		kill -TERM $$' \
		'		builtin printf "early stop: still running\n" >&2' '		;;' '	esac' '	builtin printf "$@"' '}' >"$work/printf-term.env"
	printf '%s\n' 'unset BASH_ENV' 'printf() {' '	case "${1:-}" in' "	'check-no-secrets: ok'*) : \"\$xfx_test_unset\" ;;" \
		'	esac' '	builtin printf "$@"' '}' >"$work/printf-unset.env"
}
for row in "check-preview-contract.sh|actionlint|^check-preview-contract: ok" \
	"test-check-no-secrets.sh|printf-term|^test-check-no-secrets: ok \\(" \
	"check-no-secrets.sh|printf-unset|^check-no-secrets: ok"; do
	name=${row%%|*}
	rest=${row#*|}
	env_file=${rest%%|*}
	done_re=${rest#*|}
	if [ "$want" != "  " ] && [ "${want#* "$name" }" = "$want" ]; then
		continue
	fi
	log="$work/late-$name.log"
	rm -f "$work/marks"
	start=$SECONDS
	(cd "$repo" && BASH_ENV="$work/$env_file.env" "$bash_bin" "scripts/$name") </dev/null >"$log" 2>&1
	status=$?
	if [ -e "$work/marks" ]; then cat "$work/marks" >>"$log"; fi
	finished=no
	if grep -q -E "$done_re" "$log"; then finished=yes; fi
	printf '%-26s %-12s exit=%-3s fired=%s ran-on-after-signal=%s unbound-msg=%s finished-line=%s %ss\n' \
		"$name" "$env_file" "$status" "$(grep -c -e '^early stop: SIGTERM' -e 'xfx_test_unset: unbound variable' "$log")" \
		"$(grep -c '^early stop: still running' "$log")" "$(grep -c 'xfx_test_unset: unbound variable' "$log")" \
		"$finished" "$((SECONDS - start))"
done
