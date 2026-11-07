#!/usr/bin/env bash
#
# SECRETS-ARGMAX's platform receipt: the platform, the measurements the
# receipt quotes, the check on the repository, the test, and the test with
# the base's and round 1's check.
#
#   platform.sh <copy> <base rev> <round-1 rev>
#
# Run it on a DISPOSABLE copy -- a `git clone --no-local`, or a container's
# copy -- never on a worktree: it overwrites the copy's
# scripts/check-no-secrets.sh to run the older checks, and puts it back.

set -uo pipefail

copy=$1 base=$2 round1=$3
cd "$copy" || exit 1
if [ -n "$(git status --porcelain)" ]; then
	echo "platform.sh: the copy is not clean; refusing"
	exit 1
fi
unset GREP_OPTIONS

section() {
	printf '\n== %s\n' "$1"
}

# Runs "$@", then prints its exit status and its time in whole seconds.
timed() {
	local start status=0
	start=$(date +%s)
	"$@" </dev/null || status=$?
	printf -- '-> exit %d, %d s\n' "$status" $(($(date +%s) - start))
}

section "platform"
printf 'head %s\n' "$(git rev-parse --short HEAD)"
uname -sm
bash --version | head -1
grep --version 2>&1 | head -1
printf 'ARG_MAX %s\n' "$(getconf ARG_MAX)"
printf 'uid %s\n' "$(id -u)"
printf 'LANG=%s LC_ALL=%s\n' "${LANG-unset}" "${LC_ALL-unset}"

for rev in "$base" HEAD; do
	section "the tracked names of $(git rev-parse --short "$rev") as one grep's operands"
	bytes=$(git ls-tree -r -z --name-only "$rev" | wc -c | tr -d ' ')
	count=$(git ls-tree -r -z --name-only "$rev" | tr -cd '\000' | wc -c | tr -d ' ')
	arg_max=$(getconf ARG_MAX)
	total=$((bytes + 2 * count + 8 * count))
	printf 'names with their NULs  %d\n' "$bytes"
	printf 'names                  %d\n' "$count"
	printf './ per name            %d\n' $((2 * count))
	printf 'an 8-byte pointer each %d\n' $((8 * count))
	printf 'total                  %d\n' "$total"
	printf 'ARG_MAX                %d\n' "$arg_max"
	printf 'room                   %d\n' $((arg_max - total))
done

section "\${#name} in a UTF-8 locale, under local LC_ALL=C, and after"
env -u LC_ALL LANG=C.UTF-8 bash -c '
	x="$(printf "caf\303\251")"
	f() {
		local LC_ALL=C
		printf "in local LC_ALL=C: %d\n" "${#x}"
	}
	printf "LANG=%s: %d\n" "$LANG" "${#x}"
	f
	printf "after the function returns: %d\n" "${#x}"
'

section "the status bash gives an exec that fails with E2BIG"
bash -c '
	a=()
	pad="$(printf "%200s" "")"
	for i in $(seq 1 40000); do a+=("${pad// /x}$i"); done
	s=0; env true "${a[@]}" 2>/dev/null || s=$?
	printf "no set -e, a command:               %d\n" "$s"
	t=0; h="$(env true "${a[@]}" 2>&1)" || t=$?
	printf "no set -e, a command substitution:  %d (%s)\n" "$t" "$h"
	set -e
	s=0; env true "${a[@]}" 2>/dev/null || s=$?
	printf "set -e, a command:                  %d\n" "$s"
	t=0; h="$(env true "${a[@]}" 2>&1)" || t=$?
	printf "set -e, a command substitution:     %d\n" "$t"
'

section "GREP_OPTIONS=--exclude=*.md and this grep"
probe="$(mktemp -d "${TMPDIR:-/tmp}/xfx-grep-options.XXXXXX")"
key="s""k-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
printf '%s\n' "$key" >"$probe/notes.md"
printf '%s\n' "$key" >"$probe/notes.txt"
(cd "$probe" && GREP_OPTIONS='--exclude=*.md' grep -H -n -E -e 's[k]-[A-Za-z0-9]{20,}' -- ./notes.md ./notes.txt) | sed 's/s[k]-A*/<fake-key>/'
printf -- '-> exit %d; notes.md and notes.txt hold the same key\n' "${PIPESTATUS[0]}"
rm -rf "$probe"

section "scripts/check-no-secrets.sh on this copy"
timed ./scripts/check-no-secrets.sh

section "the base's check ($(git rev-parse --short "$base")) on this copy"
git show "$base:scripts/check-no-secrets.sh" >scripts/check-no-secrets.sh
timed ./scripts/check-no-secrets.sh
git checkout -q -- scripts/check-no-secrets.sh

section "scripts/test-check-no-secrets.sh"
timed ./scripts/test-check-no-secrets.sh

section "RED: the test, with the base's check ($(git rev-parse --short "$base"))"
git show "$base:scripts/check-no-secrets.sh" >scripts/check-no-secrets.sh
timed ./scripts/test-check-no-secrets.sh
git checkout -q -- scripts/check-no-secrets.sh

section "the test, with round 1's check ($(git rev-parse --short "$round1"))"
git show "$round1:scripts/check-no-secrets.sh" >scripts/check-no-secrets.sh
timed ./scripts/test-check-no-secrets.sh
git checkout -q -- scripts/check-no-secrets.sh

section "the copy afterwards"
if [ -z "$(git status --porcelain)" ]; then
	echo "clean"
else
	git status --porcelain
fi
