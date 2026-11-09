#!/usr/bin/env bash
# hook-probe2.sh: with the decoy's config forcing the gc task (strategy gc,
# gc.auto=1, two names in objects/17, core.hooksPath), does a second
# pre-auto-gc call happen in the background after `git commit` returns, with
# objects/maintenance.lock held while it waits, and the lock gone after it
# exits? First call exits 0 at once; the second waits for "go" (bounded) and
# exits 1, so gc never runs.
set -euo pipefail
root="$(mktemp -d "${TMPDIR:-/tmp}/hook-probe2.XXXXXX")"
trap 'rm -rf "$root"' EXIT
now() { perl -MTime::HiRes=time -e 'printf "%.3f", time'; }
sg() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false ${GUARD:+-c maintenance.auto=false -c gc.auto=0} "$@"; }
d="$root/decoy"
hooks="$root/hooks"
mkdir -p "$d" "$hooks"
cat >"$hooks/pre-auto-gc" <<EOF
#!/bin/sh
n=\$(( \$(cat "$root/calls" 2>/dev/null || echo 0) + 1 ))
printf '%s\n' "\$n" >"$root/calls"
printf 'call %s start %s ppid-cmd: %s\n' "\$n" "\$(perl -MTime::HiRes=time -e 'printf "%.3f", time')" "\$(ps -o command= -p \$PPID | cut -c1-60)" >>"$root/hooklog"
[ "\$n" -lt 2 ] && exit 0
i=0
while [ "\$i" -lt 200 ] && [ ! -e "$root/go" ]; do sleep 0.05; i=\$((i + 1)); done
printf 'call %s exit %s\n' "\$n" "\$(perl -MTime::HiRes=time -e 'printf "%.3f", time')" >>"$root/hooklog"
exit 1
EOF
chmod 755 "$hooks/pre-auto-gc"
sg -C "$d" init -q
sg -C "$d" config gc.auto 1
sg -C "$d" config maintenance.strategy gc
sg -C "$d" config core.hooksPath "$hooks"
mkdir -p "$d/.git/objects/17"
: >"$d/.git/objects/17/$(printf '%038d' 1)"
: >"$d/.git/objects/17/$(printf '%038d' 2)"
printf 'decoy\n' >"$d/d.txt"
sg -C "$d" add -- d.txt
printf 'commit start %s\n' "$(now)"
sg -C "$d" -c user.name=decoy -c user.email=decoy@example.invalid -c commit.gpgsign=false commit -q -m decoy
printf 'commit returned %s\n' "$(now)"
sleep 0.5
printf 'commit+0.5s %s: lock=%s\n' "$(now)" "$([ -e "$d/.git/objects/maintenance.lock" ] && echo yes || echo no)"
(cd "$d/.git" && find . -newer "$d/d.txt" -type f | sort | sed 's/^/  new: /')
: >"$root/go"
printf 'go %s\n' "$(now)"
i=0
while [ "$i" -lt 200 ] && [ -e "$d/.git/objects/maintenance.lock" ]; do sleep 0.05; i=$((i + 1)); done
printf 'lock gone after %d polls %s\n' "$i" "$(now)"
sleep 0.2
cat "$root/hooklog" 2>/dev/null || echo 'hook never ran'
(cd "$d/.git" && find . -newer "$d/d.txt" -type f | sort | sed 's/^/  now: /')
