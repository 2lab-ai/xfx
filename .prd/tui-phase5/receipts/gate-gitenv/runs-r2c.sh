#!/usr/bin/env bash
#
# GATE-GITENV round 2c's RED and GREEN on one platform, on a disposable clone:
#   - scripts/test-gate-env.sh at 625abcf1 (2b's head) for its count, at the
#     2c test commit (the gates at 625abcf1) and at the 2c fix commit;
#   - discovery-probe.sh with the identity script at 625abcf1 and at the fix.
#
#   runs-r2c.sh <clone> <tools dir> <test commit> <fix commit>
set -uo pipefail
clone=$1 tools=$2 test_rev=$3 fix_rev=$4
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

for rev in 625abcf1 "$test_rev" "$fix_rev"; do
	at "$rev"
	printf '\n== scripts/test-gate-env.sh at %s\n' "$(git rev-parse --short HEAD)"
	timed ./scripts/test-gate-env.sh
done
for rev in 625abcf1 "$fix_rev"; do
	at "$rev"
	printf '\n'
	timed bash "$tools/discovery-probe.sh" "$clone"
done
at "$fix_rev"
printf '\n== the clone afterwards\n'; if [ -z "$(git status --porcelain)" ]; then echo clean; else git status --porcelain; fi
