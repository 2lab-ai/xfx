#!/usr/bin/env bash
#
# GATE-GITENV round 2b: a copy of xfx with no .git of its own, inside a parent
# repository that tracks it (a vendored subtree). The parent tracks
# vendor/xfx/Cargo.lock holding the retired name and the identity script at
# vendor/xfx/scripts/. Each row runs the copy's script from <checkout>.
#
#   vendored-probe.sh <checkout> [label]
set -uo pipefail
src=$1
label=${2:-}
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done
unset GREP_OPTIONS
s="$(mktemp -d "${TMPDIR:-/tmp}/xfx-vendored.XXXXXX")"
trap 'rm -rf "$s"' EXIT
sg() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c user.name=p -c user.email=p@example.invalid -c commit.gpgsign=false "$@"; }
retired="f""x""r"
printf '== the identity script at %s%s\n' "$(git -C "$src" rev-parse --short HEAD)" "${label:+ ($label)}"
uname -sm; git --version
row() { printf '%-52s exit %-3s %s\n' "$1" "$2" "$3"; }

parent="$s/parent"
mkdir -p "$parent/vendor/xfx/scripts"
cp "$src/scripts/check-xfx-identity.sh" "$parent/vendor/xfx/scripts/"
printf 'name = "%s-core"\n' "$retired" >"$parent/vendor/xfx/Cargo.lock"
printf 'parent\n' >"$parent/README"
sg -C "$parent" init -q
sg -C "$parent" add -A
sg -C "$parent" commit -q -m parent
st=0; "$parent/vendor/xfx/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null || st=$?
row "vendored, the parent tracks the residue" "$st" "$(grep -m1 -E 'still|could not|cannot|ok \(|checkout' "$s/out" | cut -c1-110)"
# The same copy with a repository of its own, as a control.
own="$s/own"
mkdir -p "$own/scripts"
cp "$src/scripts/check-xfx-identity.sh" "$own/scripts/"
printf 'name = "%s-core"\n' "$retired" >"$own/Cargo.lock"
sg -C "$own" init -q
sg -C "$own" add -A
st=0; "$own/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null || st=$?
row "its own repository (control)" "$st" "$(grep -m1 -E 'still|could not|cannot|ok \(|checkout' "$s/out" | cut -c1-110)"
# A linked worktree of that repository: its .git is a file.
sg -C "$own" commit -q -m own
sg -C "$own" worktree add -q "$s/linked" -b linked
st=0; "$s/linked/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null || st=$?
row "a linked worktree (.git is a file)" "$st" "$(grep -m1 -E 'still|could not|cannot|ok \(|checkout' "$s/out" | cut -c1-110)"
