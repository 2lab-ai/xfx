#!/usr/bin/env bash
# trace-line.sh: the line decoy-setup-starts-nothing requires, as this git
# writes it. Runs new_decoy's calls (test-gate-env.sh :319-346 at 2a9b1c22,
# decoy_forced set, through sg as at that head) under GIT_TRACE in a
# throwaway directory, and prints every trace line, timestamps cut.
set -euo pipefail
root="$(mktemp -d "${TMPDIR:-/tmp}/trace-line.XXXXXX")"
trap 'rm -rf "$root"' EXIT
sg() {
	GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
		git -c core.fsmonitor=false -c maintenance.auto=false -c gc.auto=0 "$@"
}
decoy="$root/decoy"
mkdir -p "$decoy" "$root/maint-hooks"
export GIT_TRACE="$root/trace"
sg -C "$decoy" init -q
sg -C "$decoy" config maintenance.auto true
sg -C "$decoy" config maintenance.strategy gc
sg -C "$decoy" config gc.auto 1
sg -C "$decoy" config core.hooksPath "$root/maint-hooks"
mkdir -p "$decoy/.git/objects/17"
: >"$decoy/.git/objects/17/$(printf '%038d' 1)"
: >"$decoy/.git/objects/17/$(printf '%038d' 2)"
printf 'decoy\n' >"$decoy/d.txt"
sg -C "$decoy" add -- d.txt
sg -C "$decoy" -c user.name=decoy -c user.email=decoy@example.invalid \
	-c commit.gpgsign=false commit -q -m decoy
unset GIT_TRACE
printf '# git %s\n' "$(git --version | cut -d' ' -f3)"
sed -E 's/^[0-9:.]+ +//' "$root/trace" | sed "s#$root#<root>#g"
