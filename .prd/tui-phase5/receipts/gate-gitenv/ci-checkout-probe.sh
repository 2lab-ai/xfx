#!/usr/bin/env bash
#
# GATE-GITENV round 2d: check-xfx-identity.sh at <rev> of <clone> in a
# one-commit-deep checkout made the way actions/checkout leaves one (init,
# fetch --depth=1 of the commit, a detached checkout), and in a linked
# worktree added to that checkout.
#
#   ci-checkout-probe.sh <clone> <rev>
set -uo pipefail
clone=$1 rev=$2
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done
unset GREP_OPTIONS
s="$(mktemp -d "${TMPDIR:-/tmp}/xfx-ci-checkout.XXXXXX")"
trap 'rm -rf "$s"' EXIT
sg() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c user.name=p -c user.email=p@example.invalid -c commit.gpgsign=false "$@"; }
full="$(git -C "$clone" rev-parse "$rev")"
printf '== check-xfx-identity.sh at %s\n' "${full:0:8}"
uname -sm; git --version
sg init -q "$s/checkout"
sg -C "$s/checkout" remote add origin "file://$clone"
sg -C "$s/checkout" -c protocol.file.allow=always fetch -q --no-tags --depth=1 origin "$full"
sg -C "$s/checkout" checkout -q --detach FETCH_HEAD
printf 'checkout: HEAD %s, shallow %s, .git a %s\n' "$(sg -C "$s/checkout" rev-parse --short HEAD)" \
	"$(sg -C "$s/checkout" rev-parse --is-shallow-repository)" "$([ -d "$s/checkout/.git" ] && echo directory || echo file)"
st=0; "$s/checkout/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null || st=$?
printf '%-44s exit %-3s %s\n' "one-commit-deep checkout" "$st" "$(tail -n 1 "$s/out" | cut -c 1-90)"
sg -C "$s/checkout" worktree add -q --detach "$s/linked" HEAD
printf 'linked worktree: .git a %s\n' "$([ -d "$s/linked/.git" ] && echo directory || echo file)"
st=0; "$s/linked/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null || st=$?
printf '%-44s exit %-3s %s\n' "a linked worktree of it" "$st" "$(tail -n 1 "$s/out" | cut -c 1-90)"
