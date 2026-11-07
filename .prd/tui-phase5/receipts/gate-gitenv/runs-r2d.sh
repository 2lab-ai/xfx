#!/usr/bin/env bash
#
# GATE-GITENV round 2d's RED and GREEN on one platform, on a disposable clone:
#   - scripts/test-gate-env.sh at 978ef4bf (2c's head) for its count, at the
#     2d test commit (the gates at 978ef4bf), and at <head>;
#   - the head's test-gate-env.sh against the gates at 978ef4bf, for the
#     case added after the fix;
#   - discovery-probe.sh with the identity script at 978ef4bf and at <head>.
#
#   runs-r2d.sh <clone> <tools dir> <test commit> <head>
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

for rev in 978ef4bf "$test_rev" "$head_rev"; do
	at "$rev"
	printf '\n== scripts/test-gate-env.sh at %s\n' "$(git rev-parse --short HEAD)"
	timed ./scripts/test-gate-env.sh
done
at 978ef4bf
git show "$head_rev:scripts/test-gate-env.sh" >scripts/test-gate-env.sh
printf '\n== scripts/test-gate-env.sh from %s, the gates at %s\n' "$(git rev-parse --short "$head_rev")" "$(git rev-parse --short HEAD)"
timed ./scripts/test-gate-env.sh
git checkout -q -- scripts/test-gate-env.sh
for rev in 978ef4bf "$head_rev"; do
	at "$rev"
	printf '\n'
	timed bash "$tools/discovery-probe.sh" "$clone"
done
at "$head_rev"
printf '\n'
timed bash "$tools/ci-checkout-probe.sh" "$clone" "$head_rev"
printf '\n== the clone afterwards\n'; if [ -z "$(git status --porcelain)" ]; then echo clean; else git status --porcelain; fi
