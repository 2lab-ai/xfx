#!/usr/bin/env bash
# LOCAL-PATHS-EXIT-TRAP: each changed script's normal run, the head's version
# and the base's, on the same tree (a throwaway clone at the head, the base's
# version of one script put in place for its run), and the difference.
#
#   normal.sh <clone> <base> <log-dir> <xfx binary>
#
# Each run's output has every path written `<path>` and every figure that
# changes from run to run (seconds, a pid) written `<n>`, then the two are
# compared. One summary line per script, then the differing lines. The raw
# logs stay in <log-dir>.
set -u
clone=$1 base=$2 logs=$3 binary=$4
mkdir -p "$logs"
cd "$clone" || exit 2

normalize() {
	sed -E -e 's#/[^ :,)]*#<path>#g' \
		-e 's/[0-9]+(\.[0-9]+)?(s|ms)([^A-Za-z]|$)/<n>\2\3/g' "$1"
}

echo "# head $(git rev-parse --short HEAD), base $(git rev-parse --short "$base"); /bin/bash $(/bin/bash -c 'echo $BASH_VERSION')"
for script in check-no-secrets.sh check-xfx-identity.sh check-preview-contract.sh test-check-no-secrets.sh \
	test-preview-contract.sh test-gate-env.sh smoke.sh; do
	for side in head base; do
		if [ "$side" = base ]; then
			git show "$base:scripts/$script" >"scripts/$script"
		fi
		args=()
		if [ "$script" = smoke.sh ]; then
			args=("$binary" "$logs/smoke-$side")
		fi
		/bin/bash "scripts/$script" ${args[@]+"${args[@]}"} </dev/null >"$logs/$script.$side.log" 2>&1
		echo $? >"$logs/$script.$side.status"
		normalize "$logs/$script.$side.log" >"$logs/$script.$side.norm"
		git checkout -q -- "scripts/$script"
	done
	diff "$logs/$script.base.norm" "$logs/$script.head.norm" >"$logs/$script.diff"
	printf '%s: base exit %s, head exit %s; %d line(s) only in base, %d only in head\n' "$script" \
		"$(cat "$logs/$script.base.status")" "$(cat "$logs/$script.head.status")" \
		"$(grep -c '^<' "$logs/$script.diff")" "$(grep -c '^>' "$logs/$script.diff")"
	grep -E '^[<>]' "$logs/$script.diff" | sed 's/^/    /'
done
echo "# tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
