#!/usr/bin/env bash
#
# GATE-GITENV-F: RED and GREEN on one platform, on a disposable clone:
#   - scripts/test-gate-env.sh at 621c9504 (GATE-GITENV with main merged) for its count, at the
#     F test commit (the gates at 621c9504), and at <head>;
#   - ci-checkout-probe.sh at <head>.
#
#   runs-f.sh <clone> <tools dir> <test commit> <head>
set -uo pipefail
clone=$1 tools=$2 test_rev=$3 head_rev=$4
cd "$clone" || exit 1
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done
unset GREP_OPTIONS
printf '== platform\n'; uname -sm; bash --version | head -1; git --version
ruby -v; rustc --version; printf 'uid %s\n' "$(id -u)"

timed() {
	local start status=0
	start=$(date +%s)
	"$@" </dev/null 2>&1 || status=$?
	printf -- '-> exit %d, %d s\n' "$status" $(($(date +%s) - start))
}
at() {
	git checkout -q "$1" || exit 1
}

for rev in 621c9504 "$test_rev" "$head_rev"; do
	at "$rev"
	printf '\n== scripts/test-gate-env.sh at %s\n' "$(git rev-parse --short HEAD)"
	timed ./scripts/test-gate-env.sh
done
at "$head_rev"
printf '\n'
timed bash "$tools/ci-checkout-probe.sh" "$clone" "$head_rev"
printf '\n== the clone afterwards\n'; if [ -z "$(git status --porcelain)" ]; then echo clean; else git status --porcelain; fi
