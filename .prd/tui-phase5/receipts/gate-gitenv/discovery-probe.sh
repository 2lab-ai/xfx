#!/usr/bin/env bash
#
# GATE-GITENV rounds 2c and 2d: the identity script of <checkout>, run on a
# copy with no repository of its own inside a parent that tracks the
# residue, reached four ways, and given a root .git naming the parent's
# repository two ways; on an own checkout reached four ways, with a tracked
# file deleted, and as a submodule; on a .git naming a decoy that tracks
# only clean files present here; and on an entry with both index bits, set
# by two update-index calls and by one. Each row: exit status and the first
# line it said.
#
#   discovery-probe.sh <checkout>
set -uo pipefail
src=$1
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done
unset GREP_OPTIONS
s="$(mktemp -d "${TMPDIR:-/tmp}/xfx-discovery.XXXXXX")"
trap 'rm -rf "$s"' EXIT
sg() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c user.name=p -c user.email=p@example.invalid -c commit.gpgsign=false "$@"; }
retired="f""x""r"
printf '== the identity script at %s\n' "$(git -C "$src" rev-parse --short HEAD)"
uname -sm; git --version; bash --version | head -1
row() {
	local label=$1 status=0
	shift
	"$@" >"$s/out" 2>&1 </dev/null || status=$?
	printf '%-44s exit %-3s %s\n' "$label" "$status" "$(grep -m1 -E 'check-xfx-identity: ' "$s/out" | cut -c 21-130)"
}
vendor() {
	mkdir -p "$1/vendor/xfx/scripts"
	cp "$src/scripts/check-xfx-identity.sh" "$1/vendor/xfx/scripts/"
	printf 'name = "%s-core"\n' "$retired" >"$1/vendor/xfx/Cargo.lock"
	sg -C "$1" init -q
	sg -C "$1" add -A
}
own() {
	mkdir -p "$1/scripts"
	cp "$src/scripts/check-xfx-identity.sh" "$1/scripts/"
	printf 'name = "%s-core"\n' "$retired" >"$1/Cargo.lock"
	sg -C "$1" init -q
	sg -C "$1" add -A
}
vendor "$s/vendored"
row "vendored, by its path" "$s/vendored/vendor/xfx/scripts/check-xfx-identity.sh"
ln -s "$s/vendored/vendor/xfx" "$s/xfx-link"
row "vendored, through a symlink to its root" "$s/xfx-link/scripts/check-xfx-identity.sh"
vendor "$s/vend:ored"
row "vendored, a ':' in the parent's path" "$s/vend:ored/vendor/xfx/scripts/check-xfx-identity.sh"
if [ "$s/vendored" -ef "$s/VENDORED" ]; then
	row "vendored, by a path in another case" "$s/VENDORED/vendor/xfx/scripts/check-xfx-identity.sh"
else
	printf '%-44s skip (this file system does not fold case)\n' "vendored, by a path in another case"
fi
own "$s/own"
row "own, by its path" "$s/own/scripts/check-xfx-identity.sh"
ln -s "$s/own" "$s/own-link"
row "own, through a symlink to its root" "$s/own-link/scripts/check-xfx-identity.sh"
if [ "$s/own" -ef "$s/OWN" ]; then
	row "own, by a path in another case" "$s/OWN/scripts/check-xfx-identity.sh"
else
	printf '%-44s skip (this file system does not fold case)\n' "own, by a path in another case"
fi
sg -C "$s/own" commit -q -m own
sg -C "$s/own" worktree add -q "$s/linked" -b linked
row "own, a linked worktree" "$s/linked/scripts/check-xfx-identity.sh"
# 2d: a root .git naming the parent's repository.
vendor "$s/vgitfile"
printf 'gitdir: %s\n' "$s/vgitfile/.git" >"$s/vgitfile/vendor/xfx/.git"
row "vendored, its .git a file naming the parent's" "$s/vgitfile/vendor/xfx/scripts/check-xfx-identity.sh"
vendor "$s/vgitlink"
ln -s "$s/vgitlink/.git" "$s/vgitlink/vendor/xfx/.git"
row "vendored, its .git a symlink to the parent's" "$s/vgitlink/vendor/xfx/scripts/check-xfx-identity.sh"
own "$s/deleted"
rm -f "$s/deleted/Cargo.lock"
row "own, its tracked Cargo.lock deleted" "$s/deleted/scripts/check-xfx-identity.sh"
own "$s/subsrc"
sg -C "$s/subsrc" commit -q -m subsrc
mkdir -p "$s/super"
sg -C "$s/super" init -q
sg -C "$s/super" -c protocol.file.allow=always submodule --quiet add "$s/subsrc" sub
row "own, a submodule" "$s/super/sub/scripts/check-xfx-identity.sh"
# The trust boundary: a .git naming another repository whose tracked paths
# all exist here. What that repository tracks is what is scanned.
mkdir -p "$s/decoy/scripts"
cp "$src/scripts/check-xfx-identity.sh" "$s/decoy/scripts/"
sg -C "$s/decoy" init -q
sg -C "$s/decoy" add scripts/check-xfx-identity.sh
mkdir -p "$s/planted/scripts"
cp "$src/scripts/check-xfx-identity.sh" "$s/planted/scripts/"
printf 'name = "%s-core"\n' "$retired" >"$s/planted/Cargo.lock"
printf 'gitdir: %s\n' "$s/decoy/.git" >"$s/planted/.git"
row "a .git naming a decoy tracking the clean files" "$s/planted/scripts/check-xfx-identity.sh"
own "$s/both"
sg -C "$s/both" update-index --skip-worktree Cargo.lock
sg -C "$s/both" update-index --assume-unchanged Cargo.lock
printf 'ls-files -v, two update-index calls: %s\n' "$(sg -C "$s/both" ls-files -v Cargo.lock)"
own "$s/onecall"
sg -C "$s/onecall" update-index --skip-worktree --assume-unchanged Cargo.lock
printf 'ls-files -v, one call with both options: %s\n' "$(sg -C "$s/onecall" ls-files -v Cargo.lock)"
"$s/both/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null
printf '%-44s exit %-3s %s\n' "both index bits on Cargo.lock" "$?" "$(grep -c -E '^check-xfx-identity: tracked files marked' "$s/out") refusal line(s): $(grep -o -E 'marked (skip-worktree|assume-unchanged)' "$s/out" | tr '\n' ' ')"
# What `pwd -P`, the fix round 2b's reviewers suggested, gives for the root
# when the script is reached by a path in another case: bash's, and the
# system's getcwd (python's os.getcwd), which is what git compares with.
if [ "$s/vendored" -ef "$s/VENDORED" ]; then
	got="$(cd "$s/VENDORED/vendor/xfx" && pwd -P)"
	sys="$(cd "$s/VENDORED/vendor/xfx" && python3 -I -c 'import os; print(os.getcwd())')"
	which() {
		case "$1" in
		*VENDORED*) echo "keeps VENDORED" ;;
		*) echo "gives vendored" ;;
		esac
	}
	printf 'bash pwd -P, root reached as .../VENDORED/...: %s\n' "$(which "$got")"
	printf 'getcwd, root reached as .../VENDORED/...:      %s\n' "$(which "$sys")"
fi
