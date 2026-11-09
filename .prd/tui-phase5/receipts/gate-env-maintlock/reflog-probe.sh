#!/usr/bin/env bash
# reflog-probe.sh: can a repository's own config hold a background
# maintenance run, with its lock, for a known time, by config alone and
# without gc? Forcing: maintenance.auto=true, the reflog-expire task enabled
# with auto=-1, core.filesRefLockTimeout=2000, and a second branch whose ref
# lock is already taken. Per flag set: how long the commit took, and whether
# objects/maintenance.lock is there 0.3 s after the commit returned and gone
# 3 s later.
set -euo pipefail
root="$(mktemp -d "${TMPDIR:-/tmp}/reflog-probe.XXXXXX")"
trap 'rm -rf "$root"' EXIT
now() { perl -MTime::HiRes=time -e 'printf "%.3f", time'; }
sg() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false "$@"; }
for flags in none both maintenance.auto=false gc.auto=0; do
	d="$root/$flags"
	mkdir -p "$d"
	sg -C "$d" init -q
	printf 'decoy\n' >"$d/d.txt"
	sg -C "$d" add -- d.txt
	sg -C "$d" -c maintenance.auto=false -c user.name=decoy -c user.email=decoy@example.invalid \
		-c commit.gpgsign=false commit -q -m first
	sg -C "$d" -c maintenance.auto=false branch held
	sg -C "$d" config maintenance.auto true
	sg -C "$d" config maintenance.reflog-expire.enabled true
	sg -C "$d" config maintenance.reflog-expire.auto -1
	sg -C "$d" config core.filesRefLockTimeout 2000
	: >"$d/.git/refs/heads/held.lock"
	printf 'decoy 2\n' >"$d/d.txt"
	sg -C "$d" add -- d.txt
	case "$flags" in
	none) guard=() ;;
	both) guard=(-c maintenance.auto=false -c gc.auto=0) ;;
	*) guard=(-c "$flags") ;;
	esac
	t0="$(now)"
	GIT_TRACE="$root/trace" sg -C "$d" ${guard[@]+"${guard[@]}"} -c user.name=decoy \
		-c user.email=decoy@example.invalid -c commit.gpgsign=false commit -q -m second
	t1="$(now)"
	sleep 0.3
	early=no
	if [ -e "$d/.git/objects/maintenance.lock" ]; then early=yes; fi
	sleep 3
	late=no
	if [ -e "$d/.git/objects/maintenance.lock" ]; then late=yes; fi
	starts="$(grep -cE 'run_command: git (maintenance run|gc --auto)' "$root/trace" || true)"
	rm -f "$root/trace"
	printf '%-24s starts=%s commit=%.3fs lock@+0.3s=%-3s lock@+3.3s=%s\n' \
		"$flags" "$starts" "$(echo "$t1 - $t0" | bc)" "$early" "$late"
done
