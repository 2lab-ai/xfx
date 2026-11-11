#!/bin/sh
# LOCAL-PATHS-EXIT-TRAP's probe of bash itself: what exit status a script
# file run by <bash> has after each kind of early stop, under each shape of
# trap the repository's scripts use.
#
#   probe.sh <bash> <scratch dir>
#
# Every script it writes is `set -euo pipefail` and makes one temporary file
# first. One line per run: the trap, the stop, the exit status, and whether
# the script reached its last line. No path is printed.
bash_bin=$1 dir=$2
mkdir -p "$dir"
printf '# bash %s\n' "$("$bash_bin" -c 'printf %s "$BASH_VERSION"')"

# run <name>: runs $dir/<name>.sh, prints its status and whether it ended.
run() {
	out="$("$bash_bin" "$dir/$1.sh" "$dir/$1.dir" 2>"$dir/$1.err")"
	rc=$?
	end=no
	[ "$out" = reached-the-end ] && end=yes
}

printf '\n## each early stop, under each trap shape (no trap; rm; rm on EXIT HUP INT TERM; a cleanup function;\n## the guarded EXIT trap this unit installs)\n'
for trap in none rm rm-sig func guarded; do
	for stop in unset-top unset-array unset-func unset-local unset-cmdsubst colon-q false-top false-func pipefail exit3-top exit3-func arith sigterm-self; do
		{
			printf '%s\n' 'set -euo pipefail' 'f="$(mktemp "${TMPDIR:-/tmp}/probe.XXXXXX")"'
			case $trap in
			none) printf '%s\n' 'rm -f "$f"' ;;
			rm) printf '%s\n' "trap 'rm -f \"\$f\"' EXIT" ;;
			rm-sig) printf '%s\n' "trap 'rm -f \"\$f\"' EXIT HUP INT TERM" ;;
			func) printf '%s\n' 'cleanup() { rm -f "$f"; }' 'trap cleanup EXIT' ;;
			guarded) printf '%s\n' 'finished=0' "trap 'stopped=\$?; rm -f \"\$f\"; [ \"\$finished\" = 1 ] || exit \"\$((stopped == 0 ? 1 : stopped))\"' EXIT" ;;
			esac
			case $stop in
			unset-top) printf '%s\n' 'echo "$nope"' ;;
			unset-array) printf '%s\n' 'a=()' 'for x in "${a[@]}"; do :; done' ;;
			unset-func) printf '%s\n' 'g() { echo "$nope"; }' 'g' ;;
			unset-local) printf '%s\n' 'g() { local x="$nope"; }' 'g' ;;
			unset-cmdsubst) printf '%s\n' 'x="$(echo "$nope")"' ;;
			colon-q) printf '%s\n' ': "${nope:?is unset}"' ;;
			false-top) printf '%s\n' 'false' ;;
			false-func) printf '%s\n' 'g() { false; echo not-here; }' 'g' ;;
			pipefail) printf '%s\n' 'false | cat' ;;
			exit3-top) printf '%s\n' 'exit 3' ;;
			exit3-func) printf '%s\n' 'g() { exit 3; }' 'g' ;;
			arith) printf '%s\n' 'x=$((1 / 0))' ;;
			sigterm-self) printf '%s\n' 'kill -TERM $$' 'sleep 0' ;;
			esac
			printf '%s\n' 'echo reached-the-end'
			[ "$trap" = guarded ] && printf '%s\n' 'finished=1'
		} >"$dir/m-$trap-$stop.sh"
		run "m-$trap-$stop"
		printf '%-7s %-15s exit=%-3s reached-end=%s\n' "$trap" "$stop" "$rc" "$end"
	done
done

printf '\n## what $? the EXIT trap sees, after an unset variable and after exit 3 in a function\n'
printf '%s\n' 'set -euo pipefail' "trap 'echo \"trap sees \$?\" >&2' EXIT" 'echo "$nope"' >"$dir/entry-unset.sh"
printf '%s\n' 'set -euo pipefail' "trap 'echo \"trap sees \$?\" >&2' EXIT" 'g() { exit 3; }' 'g' >"$dir/entry-exit3.sh"
for name in entry-unset entry-exit3; do
	run "$name"
	printf '%-12s exit=%s %s\n' "$name" "$rc" "$(grep '^trap sees' "$dir/$name.err")"
done

printf '\n## a signal sent to the script itself, between commands and while a child runs\n'
printf '## (traps: rm on EXIT only; the guarded EXIT trap this unit installs, with no trap on a signal)\n'
for shape in exit-only guarded; do
	for when in between child; do
		for sig in TERM INT HUP; do
			name="s-$shape-$when-$sig"
			{
				printf '%s\n' 'set -euo pipefail' 'd="$(mktemp -d "${TMPDIR:-/tmp}/probe.XXXXXX")"' 'echo "$d" >"$1"'
				case $shape in
				exit-only) printf '%s\n' "trap 'rm -rf \"\$d\"; echo exit-trap-ran >&2' EXIT" ;;
				guarded) printf '%s\n' 'finished=0' "trap 'stopped=\$?; rm -rf \"\$d\"; echo exit-trap-ran >&2; [ \"\$finished\" = 1 ] || exit \"\$((stopped == 0 ? 1 : stopped))\"' EXIT" ;;
				esac
				case $when in
				between) printf '%s\n' "kill -$sig \$\$" 'sleep 0' ;;
				child) printf '%s\n' "(sleep 0.3; kill -$sig \$\$) &" 'sleep 2' ;;
				esac
				printf '%s\n' 'echo reached-the-end'
				[ "$shape" = guarded ] && printf '%s\n' 'finished=1'
			} >"$dir/$name.sh"
			run "$name"
			gone=no
			[ -e "$(cat "$dir/$name.dir")" ] || gone=yes
			printf '%-9s %-7s %-4s exit=%-3s reached-end=%-3s exit-trap-ran=%s scratch-removed=%s\n' "$shape" "$when" "$sig" "$rc" "$end" \
				"$(grep -c '^exit-trap-ran' "$dir/$name.err")" "$gone"
		done
	done
done
