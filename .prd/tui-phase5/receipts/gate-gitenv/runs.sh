#!/usr/bin/env bash
#
# GATE-GITENV's RED and GREEN on one platform, on a disposable clone:
#   - scripts/test-gate-env.sh at a67881dd (the git cases, gates as at the
#     base) and at the head;
#   - the sweep (gitenv-sweep.sh) at 45d99f52 and at the head;
#   - the hook probe (hook-probe.sh) with the check at 45d99f52 and at the head.
#
#   runs.sh <clone> <tools dir>
set -uo pipefail
clone=$1 tools=$2
cd "$clone" || exit 1
for name in $(compgen -e); do
	case "$name" in
	GIT_*) unset "$name" ;;
	esac
done
unset GREP_OPTIONS
head_rev="$(git rev-parse --short HEAD)"
printf '== platform\n'; uname -sm; bash --version | head -1; grep --version | head -1; git --version; ruby -v; printf 'uid %s\n' "$(id -u)"

timed() {
	local start status=0
	start=$(date +%s)
	"$@" </dev/null 2>&1 || status=$?
	printf -- '-> exit %d, %d s\n' "$status" $(($(date +%s) - start))
}
at() {
	git checkout -q "$1" || exit 1
}

for rev in a67881dd "$head_rev"; do
	at "$rev"
	printf '\n== scripts/test-gate-env.sh at %s\n' "$(git rev-parse --short HEAD)"
	timed ./scripts/test-gate-env.sh
done
for rev in 45d99f52 "$head_rev"; do
	at "$rev"
	printf '\n== the sweep at %s\n' "$(git rev-parse --short HEAD)"
	timed bash "$tools/gitenv-sweep.sh" "$clone"
done
for rev in 45d99f52 "$head_rev"; do
	at "$rev"
	printf '\n== the hook probe, the check at %s\n' "$(git rev-parse --short HEAD)"
	timed bash "$tools/hook-probe.sh" "$clone"
done
at "$head_rev"
printf '\n== the clone afterwards\n'; if [ -z "$(git status --porcelain)" ]; then echo clean; else git status --porcelain; fi
