#!/usr/bin/env bash
# LOCAL-PATHS-EXIT-TRAP's per-script verdicts: each script that installs an
# EXIT trap, run whole, stopped by one kind of early stop right after its
# first `trap` statement, and what its exit status then is.
#
#   verdicts.sh <bash> <checkout> <work> [<script>...]
#
# <bash> runs each script (`/bin/bash`, or a bash 5 in a container); its own
# nested scripts take the bash on PATH. <checkout> is a throwaway clone at the
# commit under test: the scripts scan it and write in it. <work> holds every
# run's log, which names local paths and stays out of the receipt. One line
# per run to stdout, and nothing in it is a path.
#
# The stop is a BASH_ENV file. It unsets BASH_ENV, so only the script under
# test reads it, and defines a function `trap` that installs the trap and
# then stops the script one way:
#   set-u   reads a variable no one sets, under the script's `set -u`;
#   set-e   runs `false`, under its `set -e`;
#   exit7   runs `exit 7` inside the function;
#   term    sends SIGTERM to the script itself, then says it is still running.
# Each prints a marker line first, so a run in which the stop never fired is
# seen as such.
set -u
bash_bin=$1 repo=$2 work=$3
shift 3
mkdir -p "$work"
xfx_bin="$work/fake-xfx"
printf '#!/bin/sh\nexit 0\n' >"$xfx_bin"
chmod 755 "$xfx_bin"

# <name>|<the line that says it finished>|<arguments>
all=(
	"check-no-secrets.sh|^check-no-secrets: ok|"
	"test-check-no-secrets.sh|^test-check-no-secrets: ok \\(|"
	"check-xfx-identity.sh|^check-xfx-identity: ok|"
	"check-preview-contract.sh|^check-preview-contract: ok|"
	"test-preview-contract.sh|^test-preview-contract: ok \\(|"
	"test-gate-env.sh|^test-gate-env: ok \\(|"
	"smoke.sh|^[0-9]+ check\\(s\\), 0 failure\\(s\\)|SMOKE"
	"fxcap/capture.sh|^guards:|CAPTURE"
)
want=("$@")

for kind in ${XFX_KINDS:-set-u set-e exit7 term}; do
	env_file="$work/stop-$kind.env"
	{
		printf 'unset BASH_ENV\n'
		printf 'trap() {\n\tbuiltin trap "$@"\n'
		printf '\tprintf %s >&2\n' "'xfx-early-stop: $kind fired\\n'"
		case $kind in
		set-u) printf '\t: "$%s"\n' xfx_test_unset ;;
		set-e) printf '\tfalse\n' ;;
		exit7) printf '\texit 7\n' ;;
		term)
			printf '\tkill -TERM $$\n'
			printf '\tprintf %s >&2\n' "'xfx-early-stop: still running after SIGTERM\\n'"
			;;
		esac
		printf '}\n'
	} >"$env_file"
	for row in "${all[@]}"; do
		name=${row%%|*}
		rest=${row#*|}
		done_re=${rest%%|*}
		args=${rest#*|}
		if [ "${#want[@]}" -gt 0 ]; then
			case " ${want[*]} " in
			*" $name "*) ;;
			*) continue ;;
			esac
		fi
		log="$work/$(printf '%s' "$name" | tr / -)-$kind.log"
		run=("$bash_bin" "$repo/scripts/$name")
		case $args in
		SMOKE) run+=("$xfx_bin" "$work/smoke-$kind") ;;
		CAPTURE) run+=(xfx alias 40x120 "$work/capture-$kind") ;;
		esac
		start=$SECONDS
		(cd "$repo" && BASH_ENV="$env_file" XFX_BIN="$xfx_bin" "${run[@]}") </dev/null >"$log" 2>&1
		status=$?
		secs=$((SECONDS - start))
		fired=$(grep -c "^xfx-early-stop: $kind fired" "$log")
		after=$(grep -c '^xfx-early-stop: still running after SIGTERM' "$log")
		unbound=$(grep -c 'xfx_test_unset: unbound variable' "$log")
		finished=no
		if grep -q -E "$done_re" "$log"; then finished=yes; fi
		printf '%-26s %-6s exit=%-3s fired=%s unbound-msg=%s ran-on-after-signal=%s finished-line=%s %ss\n' \
			"$name" "$kind" "$status" "$fired" "$unbound" "$after" "$finished" "$secs"
	done
done
