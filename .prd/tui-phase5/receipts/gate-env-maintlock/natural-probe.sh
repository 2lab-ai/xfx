#!/usr/bin/env bash
# natural-probe.sh <runs> <base|fixed>: no forcing. Repeats what `refuses`
# does around a gate that never names the decoy -- new_decoy (as in
# test-gate-env.sh), the decoy's sums, 0.2 s of other work, the sums again --
# and counts the runs whose sums differ, printing the first differing lines.
# base: sg as on 32afd307; fixed: sg with -c maintenance.auto=false -c gc.auto=0.
set -euo pipefail
runs="$1"
mode="$2"
work="$(mktemp -d "${TMPDIR:-/tmp}/natural-probe.XXXXXX")"
trap 'rm -rf "$work"' EXIT
case "$mode" in
base) sg() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false "$@"; } ;;
fixed) sg() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
	git -c core.fsmonitor=false -c maintenance.auto=false -c gc.auto=0 "$@"; } ;;
*) exit 2 ;;
esac
decoy="$work/decoy"
new_decoy() {
	rm -rf "${decoy:?}"
	mkdir -p "$decoy"
	sg -C "$decoy" init -q
	printf 'decoy\n' >"$decoy/d.txt"
	sg -C "$decoy" add -- d.txt
	sg -C "$decoy" -c user.name=decoy -c user.email=decoy@example.invalid \
		-c commit.gpgsign=false commit -q -m decoy
}
decoy_sums() {
	(
		cd "$decoy/.git" || exit 1
		find . -type f -exec cksum {} +
		find . -type d | sed 's/^/dir /'
		find . -type f -perm -u+x | sed 's/^/exec /'
		find . -type l | while IFS= read -r link; do
			printf 'link %s -> %s\n' "$link" "$(readlink "$link")"
		done
	) | sort
}
differ=0
i=0
while [ "$i" -lt "$runs" ]; do
	new_decoy
	decoy_sums >"$work/before"
	sleep 0.2
	decoy_sums >"$work/after"
	if ! cmp -s "$work/before" "$work/after"; then
		differ=$((differ + 1))
		if [ "$differ" -le 3 ]; then
			printf 'run %d differs:\n' "$i"
			{ diff "$work/before" "$work/after" || true; } | sed 's/^/  /' | head -n 4
		fi
	fi
	i=$((i + 1))
done
printf 'natural-probe mode=%s runs=%d differing=%d\n' "$mode" "$runs" "$differ"
