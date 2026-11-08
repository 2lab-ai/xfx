#!/usr/bin/env bash
#
# GATE-GITENV-F on macOS: runs-f.sh on <runs clone>, then mutants-f.py's
# case-folding mutants on <mutants clone>. Run it under suite-lock.sh; each
# log's first line is the slot it holds.
#
#   mac-f.sh <tools dir> <runs clone> <mutants clone> <runs log> <mutants log> <mutant log dir>
set -uo pipefail
tools=$1 runs_clone=$2 mutants_clone=$3 runs_log=$4 mutants_log=$5 mutant_dir=$6
lock="$(bash "$tools/lock-line.sh" "$PPID")"
{
	printf '%s\n' "$lock"
	bash "$tools/runs-f.sh" "$runs_clone" "$tools" 4151a1d9 a41695bf
	echo "exit=$?"
} >"$runs_log" 2>&1
{
	printf '%s\n' "$lock"
	uname -sm; git --version; git -C "$mutants_clone" log --oneline -1
	python3 -B "$tools/mutants-f.py" "$mutants_clone" "$mutant_dir" \
		identity-own-git-failure-ignored identity-own-git-reason-dropped identity-ceiling-instead identity-own-check-by-path
	echo "mutants-exit=$?"
} >"$mutants_log" 2>&1
