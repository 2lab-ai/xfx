#!/usr/bin/env bash
# batch3.sh <worktree> <old-head> <new-head>: GATE-ENV-MAINTLOCK-F round 2.
# Run as `suite-lock.sh bash batch3.sh ...`, so the whole batch holds one
# machine-wide slot; its first line records that slot. Every mutant runs in
# a `git clone --no-local` throwaway, never in the worktree; GREEN runs in
# the worktree at its head. Each whole test-gate-env.sh is bounded by
# timeout -k 10 900 (macOS refuses ulimit -v and -d).
#
#   old-head mutants: the two new ones at the round-1 code, which the review
#     says pass there (fail open);
#   new-head mutants: the two new ones and the three of round 1;
#   setup-fails: the head with new_decoy's trace subshell made to fail, cut
#     after that section (the rest of the test is not what it tests);
#   green x3: the worktree, whole.
set -u
S="$(cd "$(dirname "$0")" && pwd)"
C="$S/clone"
W="${1:?worktree path}"
OLD="${2:?old head}"
NEW="${3:?new head}"
: "${TMPDIR:?run with TMPDIR set to a short scratch directory, as the gates need}"
mkdir -p "$TMPDIR" "$S/runs"
out="$S/runs/summary.txt"
cg() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$C" "$@"; }
slot=""
for d in "${SUITE_LOCK_ROOT:?the suite lock directory, literal when run}"/slot-*; do
	if [ "$(cat "$d/pid" 2>/dev/null)" = "$PPID" ]; then slot="$(basename "$d") since $(cat "$d/since")"; fi
done
printf '# batch3.sh started %s, git %s, bash %s, suite lock: %s\n' "$(date -u +%FT%TZ)" \
	"$(git --version | cut -d' ' -f3)" "$BASH_VERSION" "${slot:-NOT HELD}" >>"$out"
run() { # run <label> <dir> [script]
	local label="$1" dir="$2" script="${3:-./scripts/test-gate-env.sh}" t0 st=0
	t0="$(date +%s)"
	(cd "$dir" && timeout -k 10 900 "$script") >"$S/runs/$label.log" 2>&1 </dev/null || st=$?
	printf '%s head=%s exit=%d seconds=%d load=%s last=%s\n' "$label" \
		"$(git -C "$dir" rev-parse --short=8 HEAD)" "$st" $(($(date +%s) - t0)) "$(sysctl -n vm.loadavg)" \
		"$(tail -n 1 "$S/runs/$label.log")" >>"$out"
}
# mutate <from> <to>: replaces the one occurrence of <from> in the clone's
# test-gate-env.sh.
mutate() {
	ruby -e '
		path, from, to = ARGV
		text = File.read(path)
		n = text.scan(from).length
		abort "#{from.inspect} occurs #{n} times, expected once" unless n == 1
		File.write(path, text.sub(from) { to })
	' "$C/scripts/test-gate-env.sh" "$1" "$2"
}
mutant() { # mutant <label> <from> <to>
	local label="$1"
	if ! mutate "$2" "$3" 2>"$S/runs/$label.err"; then
		printf '%s NOT APPLIED: %s\n' "$label" "$(cat "$S/runs/$label.err")" >>"$out"
		return
	fi
	cg diff -- scripts/test-gate-env.sh >"$S/runs/$label.diff"
	run "$label" "$C"
	cg checkout -q -- scripts/test-gate-env.sh
}
guard='git -c core.fsmonitor=false -c maintenance.auto=false -c gc.auto=0 "$@"'
trace_on='export GIT_TRACE="$work/decoy-setup.trace"'
stale_from='mkdir -p "$decoy/.git/objects/17"'
stale_to='mkdir -p "$decoy/.git/objects/17"; : >"$decoy/.git/objects/maintenance.lock"'

cg checkout -q "$OLD"
mutant old-m4-no-trace "$trace_on" 'unset GIT_TRACE'
mutant old-m5-lock-stays "$stale_from" "$stale_to"

cg checkout -q "$NEW"
mutant m1-no-guard "$guard" 'git -c core.fsmonitor=false "$@"'
mutant m2-no-maintenance-auto "$guard" 'git -c core.fsmonitor=false -c gc.auto=0 "$@"'
mutant m3-no-gc-auto "$guard" 'git -c core.fsmonitor=false -c maintenance.auto=false "$@"'
mutant m4-no-trace "$trace_on" 'unset GIT_TRACE'
mutant m5-lock-stays "$stale_from" "$stale_to"

# new_decoy failing inside the trace subshell, the test cut right after the
# section that holds it: what it prints before it exits.
if mutate "$trace_on" "$trace_on; decoy_tracks=\"no-such-dir/\""; then
	cut_at="$(grep -n '^decoy_forced=""$' "$C/scripts/test-gate-env.sh" | sed -n 2p | cut -d: -f1)"
	head -n "$cut_at" "$C/scripts/test-gate-env.sh" >"$C/scripts/cut-setup-fails.sh"
	chmod 755 "$C/scripts/cut-setup-fails.sh"
	cg diff -- scripts/test-gate-env.sh >"$S/runs/setup-fails.diff"
	printf 'cut after line %s\n' "$cut_at" >>"$S/runs/setup-fails.diff"
	run setup-fails "$C" ./scripts/cut-setup-fails.sh
	rm -f "$C/scripts/cut-setup-fails.sh"
	cg checkout -q -- scripts/test-gate-env.sh
else
	printf 'setup-fails NOT APPLIED\n' >>"$out"
fi
printf '# clone left clean: %s\n' "$([ -z "$(cg status --porcelain)" ] && echo yes || echo no)" >>"$out"

for i in 1 2 3; do
	run "green-$i" "$W"
done
printf '# batch3.sh done %s\n' "$(date -u +%FT%TZ)" >>"$out"
