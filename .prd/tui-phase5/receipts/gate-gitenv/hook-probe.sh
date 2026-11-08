#!/usr/bin/env bash
#
# GATE-GITENV's evidence for the design point: what git exports to a hook,
# and what check-no-secrets.sh checks when a hook runs it.
#
#   hook-probe.sh <copy>       (a disposable clone; its check is the one run)
#
# A scratch repository gets a pre-commit hook that records git's variables
# and runs the copy's check, and always lets the commit through. It is
# committed four ways: plainly, with -a, with a path (a partial commit, which
# git makes from a temporary index), and from a linked worktree. Then once
# with a key staged and the working-tree file cleaned after staging: the
# commit publishes the key.

set -uo pipefail

copy=$1
check="$copy/scripts/check-no-secrets.sh"
for name in $(compgen -e); do
	case "$name" in
	GIT_*) unset "$name" ;;
	esac
done
unset GREP_OPTIONS
s="$(mktemp -d "${TMPDIR:-/tmp}/xfx-hook.XXXXXX")"
trap 'rm -rf "$s"' EXIT
key="s""k-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"

printf '== platform\nthe check: %s at %s\n' "scripts/check-no-secrets.sh" "$(git -C "$copy" rev-parse --short HEAD)"
uname -sm
git --version

g() {
	git -c user.name=probe -c user.email=probe@example.invalid -c commit.gpgsign=false "$@"
}

repo="$s/repo"
mkdir -p "$repo/scripts"
cp "$check" "$repo/scripts/check-no-secrets.sh"
printf 'clean\n' >"$repo/a.txt"
g -C "$repo" init -q
g -C "$repo" add -A
g -C "$repo" commit -q -m base
cat >"$repo/.git/hooks/pre-commit" <<'EOF'
#!/bin/sh
top="$(pwd)"
{
	printf 'hook: pwd=%s\n' "$top"
	printf 'hook: GIT_DIR=%s\n' "${GIT_DIR-<unset>}"
	printf 'hook: GIT_INDEX_FILE=%s\n' "${GIT_INDEX_FILE-<unset>}"
	printf 'hook: GIT_WORK_TREE=%s\n' "${GIT_WORK_TREE-<unset>}"
	printf 'hook: the names in GIT_INDEX_FILE: %s\n' "$(git ls-files | tr '\n' ' ')"
	status=0
	out="$(./scripts/check-no-secrets.sh 2>&1)" || status=$?
	printf 'hook: the check exits %d: %s\n' "$status" "$(printf '%s\n' "$out" | grep -m1 -E 'possible|: ok|cannot be read' )"
} >>"$HOOK_LOG"
exit 0
EOF
chmod 755 "$repo/.git/hooks/pre-commit"
export HOOK_LOG="$s/hook.log"

say() {
	printf '\n== %s\n' "$1"
	: >"$HOOK_LOG"
}
show() {
	sed -e "s#$s#<scratch>#g" "$HOOK_LOG"
}

say "git commit, a new file with a key staged"
printf '%s\n' "$key" >"$repo/b.txt"
g -C "$repo" add b.txt
g -C "$repo" commit -q -m 'b with a key'
show
printf 'published: b.txt in HEAD holds the key: %s\n' "$(g -C "$repo" show HEAD:b.txt | grep -c -F "$key")"
g -C "$repo" rm -q b.txt
g -C "$repo" commit -q --no-verify -m 'b gone'

say "git commit -a, a tracked file edited to hold a key"
printf '%s\n' "$key" >"$repo/a.txt"
g -C "$repo" commit -q -a -m 'a with a key'
show
printf 'clean\n' >"$repo/a.txt"
g -C "$repo" commit -q -a --no-verify -m 'a clean'

say "git commit -- c.txt (a partial commit) while b.txt with a key is staged"
printf 'c\n' >"$repo/c.txt"
g -C "$repo" add c.txt
printf '%s\n' "$key" >"$repo/b.txt"
g -C "$repo" add b.txt
g -C "$repo" commit -q -m 'c only' -- c.txt
show
printf 'published: HEAD has b.txt: %s\n' "$(g -C "$repo" ls-tree --name-only HEAD | grep -c -x b.txt)"
g -C "$repo" rm -q --cached b.txt
rm -f "$repo/b.txt"

say "a linked worktree: git commit there"
g -C "$repo" worktree add -q "$s/linked" -b linked
printf 'l\n' >"$s/linked/l.txt"
g -C "$s/linked" add l.txt
g -C "$s/linked" commit -q -m 'linked'
show

say "a key staged, then the working-tree file cleaned: what the commit publishes"
printf '%s\n' "$key" >"$repo/b.txt"
g -C "$repo" add b.txt
printf 'clean\n' >"$repo/b.txt"
g -C "$repo" commit -q -m 'b staged with a key, cleaned after'
show
printf 'published: b.txt in HEAD holds the key: %s\n' "$(g -C "$repo" show HEAD:b.txt | grep -c -F "$key")"
