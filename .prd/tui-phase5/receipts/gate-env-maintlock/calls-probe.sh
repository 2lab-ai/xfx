#!/usr/bin/env bash
# calls-probe.sh: which git subcommands of the decoy cases can start
# auto-maintenance? Each runs once, in the base's wrapper form
#   GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false
# (test-gate-env.sh's sg and check-xfx-identity.sh's scratch_git at 32afd307;
# check-xfx-identity.sh's scan_git is the same form but reads global config),
# under GIT_TRACE, in a repository whose
# own config asks for maintenance at every chance: the gc strategy with
# gc.auto=1 and two names in objects/17, commit-graph enabled with auto=-1.
# A line per call: the subcommand and how many `git maintenance run` or
# `git gc --auto` it started.
set -euo pipefail
root="$(mktemp -d "${TMPDIR:-/tmp}/calls-probe.XXXXXX")"
trap 'rm -rf "$root"' EXIT
base_git() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false "$@"; }
forced_repo() {
	local d="$1"
	mkdir -p "$d"
	base_git -C "$d" init -q
	base_git -C "$d" config maintenance.auto true
	base_git -C "$d" config maintenance.strategy gc
	base_git -C "$d" config gc.auto 1
	base_git -C "$d" config maintenance.commit-graph.enabled true
	base_git -C "$d" config maintenance.commit-graph.auto -1
	mkdir -p "$d/.git/objects/17"
	: >"$d/.git/objects/17/$(printf '%038d' 1)"
	: >"$d/.git/objects/17/$(printf '%038d' 2)"
}
n=0
probe() {
	local label="$1" spawned
	shift
	n=$((n + 1))
	GIT_TRACE="$root/trace.$n" "$@" >/dev/null 2>&1 || true
	spawned="$(grep -cE 'run_command: git (maintenance run|gc --auto)' "$root/trace.$n" || true)"
	printf '%-46s %s\n' "$label" "$spawned"
	grep -oE 'run_command: git (maintenance run|gc --auto)[^\n]*' "$root/trace.$n" | sed 's/^/    /' || true
}
r="$root/r"
forced_repo "$r"
printf 'decoy\n' >"$r/d.txt"
probe 'init -q <dir>' base_git init -q "$root/fresh"
probe 'config <key> <value>' base_git -C "$r" config color.grep never
probe 'add -- d.txt' base_git -C "$r" add -- d.txt
probe 'add -A -f' base_git -C "$r" add -A -f
probe 'GIT_INDEX_FILE=<f> git add <path>' env GIT_INDEX_FILE="$root/other.index" \
	GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$r" add d.txt
probe 'hash-object -w --stdin' sh -c "printf 'x\n' | GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C '$r' hash-object -w --stdin"
blob="$(printf 'x\n' | base_git -C "$r" hash-object -w --stdin)"
probe 'mktree' sh -c "printf '100644 blob $blob\tx\n' | GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C '$r' mktree"
probe 'update-index --skip-worktree' base_git -C "$r" update-index --skip-worktree d.txt
probe 'update-index --no-skip-worktree' base_git -C "$r" update-index --no-skip-worktree d.txt
probe 'grep -a --no-color -l -z -i -e <name> --' base_git -C "$r" --work-tree=. grep -a --no-color -l -z -i -e decoy --
probe 'grep -a --no-color -n -i -e <name> -- <path>' base_git -C "$r" --work-tree=. grep -a --no-color -n -i -e decoy -- ':(literal)d.txt'
probe 'ls-files' base_git -C "$r" --work-tree=. ls-files
probe 'ls-files -v -z' base_git -C "$r" --work-tree=. ls-files -v -z
probe 'rev-parse --absolute-git-dir' base_git -C "$r" --work-tree=. rev-parse --absolute-git-dir
probe '--git-dir=<.git> rev-parse --absolute-git-dir' base_git --git-dir="$r/.git" rev-parse --absolute-git-dir
probe 'diff-files --name-only -z --diff-filter=DT' base_git -C "$r" --work-tree=. diff-files --name-only -z --diff-filter=DT
probe 'commit -q -m <msg>' base_git -C "$r" -c user.name=d -c user.email=d@example.invalid \
	-c commit.gpgsign=false commit -q -m d
probe 'rev-parse HEAD' base_git -C "$r" rev-parse HEAD
probe 'worktree add -q <dir> -b <branch>' base_git -C "$r" worktree add -q "$root/linked" -b linked
s="$root/super"
mkdir -p "$s"
base_git -C "$s" init -q
probe 'submodule --quiet add <repo> sub' base_git -C "$s" -c protocol.file.allow=always submodule --quiet add "$r" sub
sleep 1
