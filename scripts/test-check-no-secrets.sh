#!/usr/bin/env bash
#
# Proves that scripts/check-no-secrets.sh reads every tracked file of a tree
# whose names do not fit on one command line, and still fails closed.
#
#   scripts/test-check-no-secrets.sh
#
# The check hands its file names to `grep` as operands, and the kernel refuses
# an `exec` whose arguments and environment together pass `ARG_MAX` (E2BIG,
# "Argument list too long"). That refusal is a failure, not a pass, so a tree
# that grows past the limit stops every merge rather than leaking anything --
# but it stops every merge. A check that splits its operands into batches is
# only as good as its batching: a batch dropped, a read failure in a batch
# other than the last one ignored, a loop that stops after the first batch,
# and the check goes on reporting `ok` over files it never opened.
#
# So this builds a scratch repository whose names overflow this machine's
# `ARG_MAX` -- and first proves that they do, by failing to `exec` with all of
# them -- copies the check into it, and requires of the check:
#   - a clean tree passes, every tracked file counted;
#   - a key in the first file, in the last, and in a file named `-n` (an option
#     if it were not `./`-qualified) is found and printed;
#   - a file in the middle that cannot be read fails the check, by name;
#   - a key in every file is found in every file: no batch is dropped, no
#     name at a batch's edge falls between two;
#   - a tree with nothing to scan is refused, not passed.
# Every run has a key on its standard input, which the check must never read,
# and none may report "Argument list too long".
#
# The keys are fakes of the shapes the check looks for, assembled at runtime,
# because this file is itself tracked and the check reads it.
#
# Requires git, getconf, and a user that cannot read a mode-000 file (not
# root).

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
check="$repo_root/scripts/check-no-secrets.sh"

# Every `git` below acts on a scratch repository named with `-C`, and on
# nothing else. One exported from a hook would point it at the repository
# that ran this.
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_OBJECT_DIRECTORY GIT_COMMON_DIR \
	GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE

work="$(mktemp -d "${TMPDIR:-/tmp}/xfx-secrets-test.XXXXXX")"
trap 'rm -rf "$work"' EXIT HUP INT TERM

failures=0
passed=0

problem() {
	printf 'test-check-no-secrets: %s\n' "$1" >&2
	failures=$((failures + 1))
}

if [ "$(id -u)" -eq 0 ]; then
	problem "running as root, which reads a mode-000 file; the unreadable case would prove nothing"
	exit 1
fi

filler="AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
key_prefix="sk"
fake_key="${key_prefix}-${filler}"
printf '%s\n' "$fake_key" >"$work/stdin"

# --- the large tree ----------------------------------------------------------
#
# Names of a fixed length, so the bytes they take as operands are known: a
# directory of 126 bytes and a file of 238, `./`-qualified, plus the NUL that
# ends each argument and the pointer to it.

arg_max="$(getconf ARG_MAX)"
case "$arg_max" in
'' | *[!0-9]*)
	problem "getconf ARG_MAX gave '$arg_max', not a number"
	exit 1
	;;
esac
# Linux with an unlimited stack reports an `ARG_MAX` it does not enforce; the
# kernel caps arguments at 6 MiB whatever the stack.
limit="$arg_max"
if [ "$limit" -gt 6291456 ]; then
	limit=6291456
fi
target=$((limit + limit / 4))

printf -v dir_pad '%*s' 120 ''
dir_pad="${dir_pad// /a}"
printf -v file_pad '%*s' 230 ''
file_pad="${file_pad// /b}"
per_file=$((2 + 6 + 120 + 1 + 8 + 230 + 1 + 8))
count=$(((target + per_file - 1) / per_file))

big="$work/big"
mkdir -p "$big/scripts"
git -C "$big" init -q
cp "$check" "$big/scripts/check-no-secrets.sh"
i=0
while [ "$i" -lt "$count" ]; do
	printf -v dir 'd%04d-%s' $((i / 100)) "$dir_pad"
	printf -v name 'f%06d-%s' "$i" "$file_pad"
	[ -d "$big/$dir" ] || mkdir "$big/$dir"
	printf 'clean %d\n' "$i" >"$big/$dir/$name"
	if [ "$i" -eq $((count / 2)) ]; then
		middle="$dir/$name"
	fi
	i=$((i + 1))
done
# Git orders names bytewise: `+` before `-` before `d` before `scripts/`
# before `zz`. So the first operand is `+first-batch`, the second `-n`, and
# the last `zz-last-batch`.
printf 'clean\n' >"$big/+first-batch"
printf 'clean\n' >"$big/-n"
printf 'clean\n' >"$big/zz-last-batch"
git -C "$big" add -f -A
tracked=$((count + 4))

git -C "$big" ls-files -z >"$work/listing"
names=()
while IFS= read -r -d '' name; do
	names+=("$name")
done <"$work/listing"
if [ "${#names[@]}" -ne "$tracked" ]; then
	problem "the large tree tracks ${#names[@]} file(s), expected $tracked"
	exit 1
fi
if [ "${names[0]}" != "+first-batch" ] || [ "${names[1]}" != "-n" ] ||
	[ "${names[$((tracked - 1))]}" != "zz-last-batch" ]; then
	problem "git does not order the large tree as expected: ${names[0]}, ${names[1]}, ..., ${names[$((tracked - 1))]}"
	exit 1
fi

# The precondition: all of these names, as operands, do not fit on one command
# line here -- the shell's `exec` fails with E2BIG, which it reports as
# "Argument list too long" in the C locale. Not by its status: bash 5 says 126,
# but bash 3.2 under `set -e` says 1. Without it a passing run below could be a
# tree the old single `grep` would have read in one go.
operands=()
for name in "${names[@]}"; do
	operands+=("./$name")
done
status=0
(LC_ALL=C && cd "$big" && env true "${operands[@]}") 2>"$work/e2big" </dev/null || status=$?
if [ "$status" -eq 0 ]; then
	problem "$tracked names of $per_file bytes each fit in ARG_MAX ($arg_max); the tree proves nothing"
	exit 1
elif ! grep -q -F -e "Argument list too long" "$work/e2big"; then
	problem "exec with every name failed with status $status, but not with E2BIG: $(sed -n '1p' "$work/e2big")"
	exit 1
fi
printf 'test-check-no-secrets: %d tracked names overflow ARG_MAX (%d): %s\n' \
	"$tracked" "$arg_max" "$(sed -n '1p' "$work/e2big")"
operands=()

# --- the cases ---------------------------------------------------------------

# Runs the check copied into $1, with a key on its standard input; leaves its
# exit status in `status` and what it printed in "$work/out".
run_check() {
	status=0
	"$1/scripts/check-no-secrets.sh" <"$work/stdin" >"$work/out" 2>&1 || status=$?
}

# verdict <case> <expected status> <text the output must contain>...
verdict() {
	local name="$1" want="$2" needle bad=""
	shift 2
	if [ "$status" -ne "$want" ]; then
		bad="the check exited $status, expected $want"
	fi
	for needle in "$@"; do
		if ! grep -q -F -e "$needle" "$work/out"; then
			bad="${bad:+$bad; }its output lacks '$needle'"
		fi
	done
	if grep -q -F -e "Argument list too long" "$work/out"; then
		bad="${bad:+$bad; }it overflowed ARG_MAX"
	fi
	if grep -q -F -e "(standard input)" "$work/out"; then
		bad="${bad:+$bad; }it read its standard input"
	fi
	if [ -n "$bad" ]; then
		problem "$name: $bad; the first lines it printed:"
		sed -n '1,12p' "$work/out" | cut -c 1-200 >&2
	else
		printf 'test-check-no-secrets: ok %-18s exit %d\n' "$name" "$status"
		passed=$((passed + 1))
	fi
}

clean() {
	printf 'clean\n' >"$big/$1"
}

run_check "$big"
verdict clean 0 "check-no-secrets: ok ($tracked tracked file(s) scanned)"

printf '%s\n' "$fake_key" >"$big/+first-batch"
run_check "$big"
verdict first-batch 1 "check-no-secrets: possible OpenAI-style key:" "./+first-batch:1:$fake_key"
clean +first-batch

printf '%s\n' "$fake_key" >"$big/zz-last-batch"
run_check "$big"
verdict last-batch 1 "check-no-secrets: possible OpenAI-style key:" "./zz-last-batch:1:$fake_key"
clean zz-last-batch

printf '%s\n' "$fake_key" >"$big/-n"
run_check "$big"
verdict dash-n-name 1 "check-no-secrets: possible OpenAI-style key:" "./-n:1:$fake_key"
clean -n

chmod 000 "$big/$middle"
if [ -r "$big/$middle" ]; then
	problem "a mode-000 file is still readable; the unreadable case proves nothing"
else
	run_check "$big"
	verdict unreadable-middle 1 \
		"check-no-secrets: grep could not read every file for the 'OpenAI-style key' pattern:" \
		"./$middle"
fi
chmod 644 "$big/$middle"

# A key in every file but the check's own copy, and every one reported.
planted=0
for name in "${names[@]}"; do
	if [ "$name" != scripts/check-no-secrets.sh ]; then
		printf '%s\n' "$fake_key" >"$big/$name"
		planted=$((planted + 1))
	fi
done
run_check "$big"
verdict every-file 1 "check-no-secrets: possible OpenAI-style key:"
found="$(grep -c -F -e ":1:$fake_key" "$work/out" || true)"
if [ "$found" -ne "$planted" ]; then
	problem "every-file: $found of the $planted planted keys were reported"
else
	printf 'test-check-no-secrets: ok %-18s %d of %d keys reported\n' every-file-count "$found" "$planted"
	passed=$((passed + 1))
fi

# --- nothing to scan ---------------------------------------------------------

# A repository that tracks nothing, and one that tracks only a symlink, which
# the check does not read: each would pass having looked at nothing. The
# check's copy is untracked in both.
empty="$work/empty"
mkdir -p "$empty/scripts"
git -C "$empty" init -q
cp "$check" "$empty/scripts/check-no-secrets.sh"
run_check "$empty"
verdict nothing-tracked 1 "check-no-secrets: no files to scan; the check would pass for the wrong reason"

links="$work/links"
mkdir -p "$links/scripts"
git -C "$links" init -q
cp "$check" "$links/scripts/check-no-secrets.sh"
ln -s nowhere "$links/link"
git -C "$links" add -f -- link
run_check "$links"
verdict only-a-symlink 1 "check-no-secrets: no files to scan; the check would pass for the wrong reason"

if [ "$failures" -ne 0 ]; then
	printf 'test-check-no-secrets: %d problem(s), %d case(s) passed\n' "$failures" "$passed" >&2
	exit 1
fi
printf 'test-check-no-secrets: ok (%d cases; %d tracked names past ARG_MAX %d)\n' "$passed" "$tracked" "$arg_max"
