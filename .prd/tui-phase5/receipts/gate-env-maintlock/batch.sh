#!/usr/bin/env bash
# batch.sh: RED, mutants and GREEN x3 for GATE-ENV-MAINTLOCK-F, run once
# under one suite-lock slot. RED and mutants run in a `git clone --no-local`
# throwaway (never the worktree); GREEN runs in the worktree at its head.
# Each test run is bounded: timeout -k 10 900 (macOS rejects ulimit -v/-d).
set -u
S="$(cd "$(dirname "$0")" && pwd)"
C="$S/clone"
W="${1:?worktree path}"
: "${TMPDIR:?run with TMPDIR set to a short scratch directory, as the gates need}"
mkdir -p "$TMPDIR" "$S/runs"
cg() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$C" "$@"; }
run() { # run <label> <dir>
	local label="$1" dir="$2" t0 t1 st=0
	t0="$(date +%s)"
	(cd "$dir" && timeout -k 10 900 ./scripts/test-gate-env.sh) >"$S/runs/$label.log" 2>&1 </dev/null || st=$?
	t1="$(date +%s)"
	printf '%s head=%s exit=%d seconds=%d load=%s\n' "$label" \
		"$(git -C "$dir" rev-parse --short=8 HEAD)" "$st" $((t1 - t0)) "$(sysctl -n vm.loadavg)" \
		>>"$S/runs/summary.txt"
}
printf '# batch.sh started %s, git %s, bash %s\n' "$(date -u +%FT%TZ)" \
	"$(git --version | cut -d' ' -f3)" "$BASH_VERSION" >>"$S/runs/summary.txt"

# RED: the case at 4cac717d, sg without the guard.
cg checkout -q 4cac717d
run red-4cac717d "$C"

# Mutants of the one changed call (sg) at d98ada7e.
cg checkout -q d98ada7e
mutant() { # mutant <label> <text to drop>
	local label="$1" drop="$2"
	if ! ruby -e '
		path, from = ARGV
		text = File.read(path)
		n = text.scan(from).length
		abort "#{from.inspect} occurs #{n} times, expected once" unless n == 1
		File.write(path, text.sub(from) { "" })
	' "$C/scripts/test-gate-env.sh" "$drop"; then
		printf '%s not applied\n' "$label" >>"$S/runs/summary.txt"
		return
	fi
	cg diff -- scripts/test-gate-env.sh >"$S/runs/$label.diff"
	run "$label" "$C"
	cg checkout -q -- scripts/test-gate-env.sh
}
mutant mutant-1-no-guard " -c maintenance.auto=false -c gc.auto=0"
mutant mutant-2-no-maintenance-auto " -c maintenance.auto=false"
mutant mutant-3-no-gc-auto " -c gc.auto=0"

# GREEN x3 on the worktree head.
for i in 1 2 3; do
	run "green-$i" "$W"
done
printf '# batch.sh done %s\n' "$(date -u +%FT%TZ)" >>"$S/runs/summary.txt"
