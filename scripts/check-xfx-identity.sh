#!/usr/bin/env bash
#
# Refuses to let the retired product name survive in the tracked tree.
#
# This product is `xfx`. It is a port of `fx`, and `fx` keeps its own name
# everywhere it is cited: the upstream repository, the upstream profile and
# project files, the upstream commands, the attribution. What must not survive
# is the *old local* name this port carried before it was called xfx. A tree
# that still spells it teaches two identities, and a reader who follows the
# older one reaches a profile directory, an environment variable, a crate, or a
# download that does not exist.
#
# The scan is over *tracked* files only -- `git ls-files` and their contents --
# because those are what a push publishes. `.git`, `target`, scratch evidence,
# the artifacts of runs that already happened, and the name of the directory
# this checkout happens to sit in are history or local accident rather than
# product surface. A check that failed on those would teach people to add
# exceptions, and an exception is how residue survives.
#
# There is no allowlist. The upstream name is `fx`, which does not match, so
# every remaining hit is local residue whatever its shape: prose, an
# identifier, a fake test literal, a comment, or a file name.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

failures=0

fail() {
	printf 'check-xfx-identity: %s\n' "$1" >&2
	failures=$((failures + 1))
}

# The retired name, assembled at runtime rather than written out.
#
# This script is itself a tracked file that the scan below reads, so a literal
# spelling here would be a finding against the file that reports findings --
# the same reason `scripts/check-no-secrets.sh` assembles its credential
# samples instead of embedding them.
retired="f"
retired="${retired}x"
retired="${retired}r"

retired_upper="$(printf '%s' "$retired" | tr '[:lower:]' '[:upper:]')"
retired_mixed="${retired_upper:0:1}${retired:1}"

# Every tracked line whose text carries the retired name, in any case.
tracked_content() {
	git -C "$1" grep -I -n -i -e "$retired" -- || true
}

# Every tracked path whose own name carries it.
tracked_paths() {
	git -C "$1" ls-files | grep -i -e "$retired" || true
}

# --- the positive control ----------------------------------------------------
#
# A check whose failure mode is "silently passes" has to prove it is awake
# before its green is worth anything: a mistyped pattern, a `git grep` that
# searched nothing, an empty file list. So the two queries above are first run
# against a throwaway repository that is deliberately dirty and must report
# every class of residue in it, and then against one carrying only the names
# that are supposed to be here, where they must report nothing.
#
# The fixtures are written from the assembled name for the same reason the name
# itself is assembled.

control_root="$(mktemp -d "${TMPDIR:-/tmp}/xfx-identity-control.XXXXXX")"
trap 'rm -rf "$control_root"' EXIT

init_repo() {
	mkdir -p "$1"
	git -C "$1" init -q
}

dirty="$control_root/dirty"
init_repo "$dirty"
mkdir -p "$dirty/docs"
printf 'The %s binary reads your workspace.\n' "$retired" >"$dirty/prose.md"
printf '%s_MODEL selects the model.\n' "$retired_upper" >"$dirty/env.md"
printf 'Writes through .%s and .%s are refused.\n' "$retired" "$retired_mixed" >"$dirty/protect.md"
printf 'The project file is .%s.json\n' "$retired" >"$dirty/project.md"
printf 'Download %s-macos-aarch64 and verify it.\n' "$retired" >"$dirty/asset.md"
printf 'https://github.com/2lab-ai/%s\n' "$retired" >"$dirty/repo.md"
printf 'This page names nothing retired.\n' >"$dirty/docs/2026-08-21-$retired-design.md"
git -C "$dirty" add -A -f

control_content="$(tracked_content "$dirty")"
control_paths="$(tracked_paths "$dirty")"

# One fixture per class, so a scan that finds only the easy lowercase prose
# cannot pass by finding one of them.
for class in prose env protect project asset repo; do
	if ! printf '%s\n' "$control_content" | grep -q "^${class}\.md:"; then
		fail "the content scan misses the '$class' residue in its own control; this scan is not working"
	fi
done

if ! printf '%s\n' "$control_paths" | grep -q '^docs/'; then
	fail 'the path scan misses a retired name in a control file name; this scan is not working'
fi

clean="$control_root/clean"
init_repo "$clean"
mkdir -p "$clean/docs"
{
	printf 'xfx is an unofficial port of `fx` (https://github.com/vercel-labs/fx).\n'
	printf 'Upstream keeps ~/.fx, .fx.json, FX_MODEL, `fx login`, and `fx setup`.\n'
	printf 'This product uses ~/.xfx, .xfx.json, XFX_MODEL, and 2lab-ai/xfx.\n'
} >"$clean/README.md"
printf 'A design page.\n' >"$clean/docs/2026-08-21-xfx-design.md"
git -C "$clean" add -A -f

if [ -n "$(tracked_content "$clean")" ] || [ -n "$(tracked_paths "$clean")" ]; then
	fail 'the scan flags upstream fx or current xfx names; it would force an allowlist onto a correct tree'
fi

# --- the scan itself ---------------------------------------------------------

if ! git rev-parse --git-dir >/dev/null 2>&1; then
	fail 'not a git checkout; this check can only speak about tracked files'
	exit 1
fi

tracked_count="$(git ls-files | wc -l | tr -d ' ')"
if [ "$tracked_count" -eq 0 ]; then
	fail 'no tracked files to scan; the check would pass for the wrong reason'
	exit 1
fi

content="$(tracked_content .)"
if [ -n "$content" ]; then
	fail 'the retired product name is still in tracked content:'
	printf '%s\n' "$content" >&2
fi

paths="$(tracked_paths .)"
if [ -n "$paths" ]; then
	fail 'the retired product name is still in tracked path names:'
	printf '%s\n' "$paths" >&2
fi

if [ "$failures" -ne 0 ]; then
	printf 'check-xfx-identity: %d problem(s) found\n' "$failures" >&2
	exit 1
fi

printf 'check-xfx-identity: ok (%s tracked file(s) scanned)\n' "$tracked_count"
