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

# Git takes where a repository is, which index it lists, where it writes
# objects and which config and attributes apply from its environment:
# `GIT_DIR`, `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`, `GIT_COMMON_DIR`,
# `GIT_CONFIG_*`, `GIT_ATTR_SOURCE` and more. One inherited from a caller --
# an exported shell, a hook -- made it pass residue in its own tree:
# with `GIT_DIR` it scanned the other repository, and with `GIT_INDEX_FILE` an
# index without the file. With `GIT_DIR`, `GIT_OBJECT_DIRECTORY` or
# `GIT_COMMON_DIR`, the controls below wrote into another repository.
# This checks the checkout it is in, as CI does: that checkout's own
# `.git`, index and working tree. So every `GIT_*` goes before any git runs.
# `${!GIT_@}` is the shell's own list of its variables so named: no command
# to be missing (`compgen` is absent from a bash built without readline, and
# a failed substitution in a `for` list does not stop `set -e`). A name that
# is no identifier cannot be unset, and git reads none such.
# scripts/test-gate-env.sh holds it to that.
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done

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

# Git for the scans, in a checkout or a control alike. Config the GIT_* drop
# leaves -- global, system, the repository's own -- can still steer it:
# - `core.fsmonitor` runs a program, and git ran it during `ls-files` and
#   `grep`. Pinned off.
# - a checkout-local `core.worktree` made `git grep` read another tree, a
#   clean copy, while the file probe below read this one. `--work-tree=.`
#   names the tree: the directory `-C` entered.
# Config that makes git fail is caught by the callers, which treat any
# failure as a failed scan.
scan_git() {
	local root="$1"
	shift
	git -c core.fsmonitor=false -C "$root" --work-tree=. "$@"
}

# Git for the throwaway control repositories: no global or system config at
# all, so no hook, template or fsmonitor of the caller's runs in them.
scratch_git() {
	GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false "$@"
}

# Every tracked line whose text carries the retired name, in any case. Prints
# the hits; returns non-zero, having said why on standard error, if the scan
# could not finish -- never an empty answer for a scan that did not run.
#
# Binary by content, not by attributes. `git grep -I` takes "binary" from the
# `diff` attribute first, and attributes come from places a caller holds: a
# global or system config's `core.attributesFile`, `.git/config`,
# `.git/info/attributes`, an untracked `.gitattributes`. `*.lock -diff` in any
# of them hid a Cargo.lock from this scan. So `-a` reads every tracked file as
# text, and a file is passed over only if it is binary by git's own rule when
# no attribute says: a NUL in its first 8000 bytes. `--no-color`, because
# `color.grep=always` would colour the names `-l -z` lists.
#
# Failure, from either `git grep`, is a status above 1 or anything on its
# standard error. Bad config (`grep.patternType`, `grep.threads`, ...) exits
# 128, and an unreadable tracked file is only `error: failed to stat` with
# status 1 -- "no match". The probe of a listed file's first bytes failing is
# a failure too.
#
# `git grep` and the probe both read the working tree only for an entry with
# neither index bit set; the scan below refuses a checkout with either.
tracked_content() {
	local root="$1" path status probe
	local listing="$control_root/listing" errors="$control_root/errors"
	status=0
	scan_git "$root" grep -a --no-color -l -z -i -e "$retired" -- >"$listing" 2>"$errors" || status=$?
	if [ "$status" -gt 1 ] || [ -s "$errors" ]; then
		printf 'check-xfx-identity: git grep could not search %s (status %d):\n' "$root" "$status" >&2
		cat "$errors" >&2
		return 1
	fi
	while IFS= read -r -d '' path; do
		if ! probe="$(head -c 8000 "$root/$path" | LC_ALL=C tr -d -c '\000' | wc -c)"; then
			printf 'check-xfx-identity: could not read the first bytes of %s\n' "$root/$path" >&2
			return 1
		fi
		if [ "$probe" -eq 0 ]; then
			status=0
			scan_git "$root" grep -a --no-color -n -i -e "$retired" -- ":(literal)$path" 2>"$errors" || status=$?
			if [ "$status" -gt 1 ] || [ -s "$errors" ]; then
				printf 'check-xfx-identity: git grep could not search %s (status %d):\n' "$root/$path" "$status" >&2
				cat "$errors" >&2
				return 1
			fi
		fi
	done <"$listing"
}

# Every tracked path whose own name carries it. Fails like tracked_content.
tracked_paths() {
	local root="$1" status=0
	local names="$control_root/names" errors="$control_root/errors"
	scan_git "$root" ls-files >"$names" 2>"$errors" || status=$?
	if [ "$status" -ne 0 ] || [ -s "$errors" ]; then
		printf 'check-xfx-identity: git ls-files could not list %s (status %d):\n' "$root" "$status" >&2
		cat "$errors" >&2
		return 1
	fi
	status=0
	grep -i -e "$retired" "$names" || status=$?
	if [ "$status" -gt 1 ]; then
		printf 'check-xfx-identity: grep could not read the names of %s\n' "$root" >&2
		return 1
	fi
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
# A run that stops before its last line fails: under bash 3.2 a script
# stopped by an error of its own (an unset variable under `set -u`) leaves
# through its EXIT trap with the trap's status, 0, and would read as a pass.
# Any exit before `finished=1` keeps the status bash gives it, or is 1 where
# bash gives 0.
finished=0
trap 'stopped=$?; rm -rf "$control_root"; [ "$finished" = 1 ] || exit "$((stopped == 0 ? 1 : stopped))"' EXIT

init_repo() {
	mkdir -p "$1"
	scratch_git -C "$1" init -q
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
scratch_git -C "$dirty" add -A -f

if ! control_content="$(tracked_content "$dirty")"; then
	fail 'the content scan could not run in its own control; this scan is not working'
fi
if ! control_paths="$(tracked_paths "$dirty")"; then
	fail 'the path scan could not run in its own control; this scan is not working'
fi

# One fixture per class, so a scan that finds only the easy lowercase prose
# cannot pass by finding one of them.
for class in prose env protect project asset repo; do
	if ! grep -q "^${class}\.md:" <<<"$control_content"; then
		fail "the content scan misses the '$class' residue in its own control; this scan is not working"
	fi
done

if ! grep -q '^docs/' <<<"$control_paths"; then
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
scratch_git -C "$clean" add -A -f

if ! clean_content="$(tracked_content "$clean")" || ! clean_paths="$(tracked_paths "$clean")"; then
	fail 'the scan could not run in its clean control; this scan is not working'
elif [ -n "$clean_content" ] || [ -n "$clean_paths" ]; then
	fail 'the scan flags upstream fx or current xfx names; it would force an allowlist onto a correct tree'
fi

# --- the scan itself ---------------------------------------------------------

# The repository git finds from here must be the one this directory's own
# `.git` names: a directory, or a file in a linked worktree or a submodule. A
# copy of xfx inside another repository (a vendored subtree) has none, and
# git walks up to the parent's; with `--work-tree=.` the parent's paths then
# resolved inside this directory, where none exists, and `git grep` skipped
# every one without a word. In practice that copy is refused because its
# `.git` cannot be opened: when it can, git's discovery starts at that same
# `.git`, and no case reaches the comparison's false side.
#
# The two are compared as files (`-ef`: device and inode), not as paths. A
# ceiling at the parent directory (GIT_CEILING_DIRECTORIES) did not hold:
# git compares it with its physical working directory, so it missed when
# this root was reached through a symlink or, on a case-folding file system,
# by a path in another case; and the list is colon-separated, so a `:` in
# the parent's path split it.
#
# This trusts the root's `.git`: what the repository it names tracks is what
# is scanned. A `.git` naming the parent's repository makes the parent's
# paths the tracked ones, and those are not in this tree: refused below, as
# tracked files missing from the working tree.
if ! discovered="$(scan_git . rev-parse --absolute-git-dir 2>"$control_root/errors")"; then
	fail 'not a git checkout, or git cannot open it; this check can only speak about the tracked files of the checkout it is in:'
	cat "$control_root/errors" >&2
	exit 1
fi
if ! own="$(git -c core.fsmonitor=false --git-dir="$repo_root/.git" rev-parse --absolute-git-dir 2>"$control_root/errors")"; then
	fail "not a git checkout of its own: the root's .git cannot be opened; this check can only speak about the tracked files of the checkout it is in:"
	cat "$control_root/errors" >&2
	exit 1
fi
if ! [ "$discovered" -ef "$own" ]; then
	fail "not a git checkout of its own: the repository found is not the root's own (git finds $discovered, the root's .git names $own)"
	exit 1
fi

# `-v` tags each entry: `H` for a plain one, `S` for skip-worktree, lower
# case for assume-unchanged (`s` for both, named in both refusals). Either
# bit makes the content scan's answer about something other than the file:
# `git grep` passes over a skip-worktree entry (a sparse checkout's file
# outside its cone) without a word, and reads an assume-unchanged one from
# the index while the probe reads the working tree, so a working copy with a
# NUL hid an index text that carried the name. Only local index state sets
# either; a CI checkout has neither. Each refusal names a remedy that ends in
# a scan of the file: clearing skip-worktree on a file the working tree lacks
# would leave a deleted file, which the scan skips, so a sparse checkout is
# to be undone.
if ! scan_git . ls-files -v -z >"$control_root/tracked" 2>"$control_root/errors" || [ -s "$control_root/errors" ]; then
	fail 'git ls-files could not list this checkout:'
	cat "$control_root/errors" >&2
	exit 1
fi
tracked_count=0
skipped=()
assumed=()
while IFS= read -r -d '' entry; do
	tracked_count=$((tracked_count + 1))
	case "$entry" in
	S\ *) skipped+=("${entry#??}") ;;
	s\ *)
		# Both bits: `git sparse-checkout disable` leaves the other.
		skipped+=("${entry#??}")
		assumed+=("${entry#??}")
		;;
	[[:lower:]]\ *) assumed+=("${entry#??}") ;;
	esac
done <"$control_root/tracked"
if [ "${#skipped[@]}" -ne 0 ] || [ "${#assumed[@]}" -ne 0 ]; then
	if [ "${#skipped[@]}" -ne 0 ]; then
		fail 'tracked files marked skip-worktree, which git grep passes over; restore them first: in a sparse checkout, git sparse-checkout disable; for a file the working tree holds, git update-index --no-skip-worktree -- <file>:'
		printf '%s\n' "${skipped[@]}" >&2
	fi
	if [ "${#assumed[@]}" -ne 0 ]; then
		fail 'tracked files marked assume-unchanged, which git grep reads from the index rather than the working tree; clear the bit: git update-index --no-assume-unchanged -- <file>:'
		printf '%s\n' "${assumed[@]}" >&2
	fi
	exit 1
fi
if [ "$tracked_count" -eq 0 ]; then
	fail 'no tracked files to scan; the check would pass for the wrong reason'
	exit 1
fi
# Every tracked file is in the working tree as the type the index records:
# `git grep` skips, without a word, a file it cannot find and an entry that
# is not a regular file. `diff-files --diff-filter=DT` lists both: `D` for a
# path that is gone or is now a directory, `T` for one whose type changed
# (a file now a symlink). `ls-files -d` saw only the first. A file deleted
# and not yet committed is refused by name too; so is every path of another
# repository a planted `.git` names.
if ! scan_git . diff-files --name-only -z --diff-filter=DT >"$control_root/deleted" 2>"$control_root/errors" ||
	[ -s "$control_root/errors" ]; then
	fail 'git diff-files could not list the tracked files missing from this checkout:'
	cat "$control_root/errors" >&2
	exit 1
fi
if [ -s "$control_root/deleted" ]; then
	fail "tracked files missing from the working tree, or there as a directory or a symlink, which git grep would skip; restore or remove them (git restore -- <file>, or git rm -- <file>), unless none of them belongs here: then this root's .git names another repository:"
	tr '\000' '\n' <"$control_root/deleted" >&2
	exit 1
fi

if ! content="$(tracked_content .)"; then
	fail 'the content scan could not finish, so it found nothing it can vouch for'
elif [ -n "$content" ]; then
	fail 'the retired product name is still in tracked content:'
	printf '%s\n' "$content" >&2
fi

if ! paths="$(tracked_paths .)"; then
	fail 'the path scan could not finish, so it found nothing it can vouch for'
elif [ -n "$paths" ]; then
	fail 'the retired product name is still in tracked path names:'
	printf '%s\n' "$paths" >&2
fi

if [ "$failures" -ne 0 ]; then
	printf 'check-xfx-identity: %d problem(s) found\n' "$failures" >&2
	exit 1
fi

printf 'check-xfx-identity: ok (%s tracked file(s) scanned)\n' "$tracked_count"
finished=1
