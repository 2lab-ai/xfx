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
#   - a `GREP_OPTIONS` that excludes Markdown does not hide a key in a
#     Markdown file (BSD grep reads that variable);
#   - a `grep` killed by a signal, or exiting 126 or 127, in a batch in the
#     middle fails the check: past 2 is "not looked at" too;
#   - a distinct key in every file is reported once each, the set compared
#     whole: no batch dropped or read twice, no name at a batch's edge lost.
#   - a tree with nothing to scan is refused, not passed;
#   - a check stopped early -- by an error of its own, an `exit`, a failed
#     subshell, or a signal, right after it installs its EXIT trap or just
#     before its ok line -- fails with that stop's status and leaves no
#     scratch file behind;
#   - a listing or a scan that an arithmetic error cut short fails.
# Most of the names carry a tab, a newline and a byte that is not UTF-8 --
# a valid two-byte character where the filesystem refuses that (macOS) -- so
# every batch boundary is between two of them.
# Every run has a key on its standard input, which the check must never read,
# and none may report "Argument list too long".
#
# The keys are fakes of the shapes the check looks for, assembled at runtime,
# because this file is itself tracked and the check reads it.
#
# Requires git, getconf, grep on `PATH`, and a user that cannot read a
# mode-000 file (not root).

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
check="$repo_root/scripts/check-no-secrets.sh"

# Every `git` below acts on a scratch repository named with `-C`, and on
# nothing else. Every `GIT_*` goes: one exported from a hook would point it at
# the repository that ran this, and `GIT_TRACE` would have it write a trace
# wherever it named. `${!GIT_@}` is the shell's own list of its variables so
# named. And `sg` runs git with no global or system config, so no hook,
# template or `core.fsmonitor` of the caller's runs in the scratch
# repositories.
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done
sg() {
	GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false "$@"
}
# And this file's own `grep`s read the check's output with options of their
# own; one case below sets `GREP_OPTIONS` for the check alone.
unset GREP_OPTIONS

work="$(mktemp -d "${TMPDIR:-/tmp}/xfx-secrets-test.XXXXXX")"
# A run that stops before its last line fails: under bash 3.2 a script
# stopped by an error of its own (an unset variable under `set -u`) leaves
# through its EXIT trap with the trap's status, 0, and would read as a pass.
# Any exit before `finished=1` keeps the status bash gives it, or is 1 where
# bash gives 0.
# No trap on HUP, INT or TERM: one there ran the cleanup and went on without
# its scratch directory; without one the shell runs this trap and dies of the
# signal.
finished=0
trap 'stopped=$?; rm -rf "$work"; [ "$finished" = 1 ] || exit "$((stopped == 0 ? 1 : stopped))"' EXIT

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

real_grep="$(type -P grep || true)"
if [ -z "$real_grep" ]; then
	problem "no grep on PATH"
	exit 1
fi

# --- the large tree ----------------------------------------------------------
#
# Names of a fixed length, so the bytes they take as operands are known: a
# directory of 126 bytes and a file of about 240, `./`-qualified, plus the NUL
# that ends each argument and the pointer to it.

tab="$(printf '\t')"
nl="$(printf '\nx')"
nl="${nl%x}"
if (printf 'x\n' >"$work/$(printf 'probe-\377')") 2>/dev/null; then
	odd="$(printf '\377')"
	odd_note="0xFF, not UTF-8"
else
	odd="$(printf '\303\251')"
	odd_note="U+00E9; this filesystem refuses a name that is not UTF-8"
fi

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
printf -v file_pad '%*s' 228 ''
file_pad="${file_pad// /b}"
# Every file name: its index, then a tab, a newline and the odd byte.
printf -v sample './d%04d-%s/f%06d-%s%s%s%s' 0 "$dir_pad" 0 "$tab" "$nl" "$odd" "$file_pad"
per_file=$(($(LC_ALL=C && printf '%d' "${#sample}") + 1 + 8))
count=$(((target + per_file - 1) / per_file))

big="$work/big"
mkdir -p "$big/scripts"
sg -C "$big" init -q
cp "$check" "$big/scripts/check-no-secrets.sh"
i=0
while [ "$i" -lt "$count" ]; do
	printf -v dir 'd%04d-%s' $((i / 100)) "$dir_pad"
	printf -v name 'f%06d-%s%s%s%s' "$i" "$tab" "$nl" "$odd" "$file_pad"
	[ -d "$big/$dir" ] || mkdir "$big/$dir"
	printf 'clean %d\n' "$i" >"$big/$dir/$name"
	if [ "$i" -eq $((count / 3)) ]; then
		marker="$dir/$name"
	fi
	if [ "$i" -eq $((count / 2)) ]; then
		middle="$dir/$name"
	fi
	i=$((i + 1))
done
# Git orders names bytewise: `+` before `-` before `d` before `notes.md`
# before `scripts/` before `zz`. So the first operand is `+first-batch`, the
# second `-n`, and the last `zz-last-batch`.
printf 'clean\n' >"$big/+first-batch"
printf 'clean\n' >"$big/-n"
printf 'clean\n' >"$big/notes.md"
printf 'clean\n' >"$big/zz-last-batch"
sg -C "$big" add -f -A
tracked=$((count + 5))

sg -C "$big" ls-files -z >"$work/listing"
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
printf 'test-check-no-secrets: %d names carry a tab, a newline and %s\n' "$count" "$odd_note"
operands=()

# --- the cases ---------------------------------------------------------------

# run_check <root> [NAME=value...]: runs the check copied into <root>, with
# those variables in its environment and a key on its standard input; leaves
# its exit status in `status` and what it printed in "$work/out".
run_check() {
	local root="$1"
	shift
	status=0
	env "$@" "$root/scripts/check-no-secrets.sh" <"$work/stdin" >"$work/out" 2>&1 || status=$?
}

# Searches the check's output, bytes as bytes: the names in it are not UTF-8.
search() {
	LC_ALL=C "$real_grep" -a "$@"
}

# verdict <case> <expected status> <text the output must contain>...
verdict() {
	local name="$1" want="$2" needle bad=""
	shift 2
	if [ "$status" -ne "$want" ]; then
		bad="the check exited $status, expected $want"
	fi
	for needle in "$@"; do
		case "$needle" in
		*"$nl"*)
			problem "$name: a needle with a newline is two needles to grep -F; this test is wrong"
			return
			;;
		esac
		if ! search -q -F -e "$needle" "$work/out"; then
			bad="${bad:+$bad; }its output lacks '$needle'"
		fi
	done
	if search -q -F -e "Argument list too long" "$work/out"; then
		bad="${bad:+$bad; }it overflowed ARG_MAX"
	fi
	if search -q -F -e "(standard input)" "$work/out"; then
		bad="${bad:+$bad; }it read its standard input"
	fi
	if [ -n "$bad" ]; then
		problem "$name: $bad; the first lines it printed:"
		LC_ALL=C sed -n '1,12p' "$work/out" | LC_ALL=C cut -c 1-200 >&2
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
	# Its name up to the newline: one line of the output.
	verdict unreadable-middle 1 \
		"check-no-secrets: grep could not read every file for the 'OpenAI-style key' pattern:" \
		"./${middle%%"$nl"*}"
fi
chmod 644 "$big/$middle"

printf '%s\n' "$fake_key" >"$big/notes.md"
run_check "$big" "GREP_OPTIONS=--exclude=*.md"
verdict grep-options 1 "check-no-secrets: possible OpenAI-style key:" "./notes.md:1:$fake_key"
clean notes.md

# A `grep` that does not finish one batch in the middle -- the one naming
# `marker` -- by a signal or by the shell's 126 or 127; every other batch,
# and every `grep` the check's own controls run, is the real one. The shim
# leaves a file behind when it fires, or the case proves nothing.
shim="$work/shim"
mkdir -p "$shim"
cat >"$shim/grep" <<EOF
#!/bin/sh
for arg in "\$@"; do
	if [ "\$arg" = "\$XFX_TEST_GREP_DIES_ON" ]; then
		: >>"\$XFX_TEST_GREP_FIRED"
		case "\$XFX_TEST_GREP_DEATH" in
		signal) kill -KILL \$\$ ;;
		*) exit "\$XFX_TEST_GREP_DEATH" ;;
		esac
	fi
done
exec "$real_grep" "\$@"
EOF
chmod 755 "$shim/grep"
for death in signal 126 127; do
	rm -f "$work/fired"
	run_check "$big" "PATH=$shim:$PATH" "XFX_TEST_GREP_DIES_ON=./$marker" \
		"XFX_TEST_GREP_DEATH=$death" "XFX_TEST_GREP_FIRED=$work/fired"
	if [ ! -e "$work/fired" ]; then
		problem "grep-$death: the shim never saw the batch it was to stop; the case proves nothing"
		continue
	fi
	verdict "grep-$death" 1 \
		"check-no-secrets: grep could not read every file for the 'OpenAI-style key' pattern:"
done

# A key of its own in every file but the check's copy, and each reported
# exactly once: the reported lines, sorted, are the expected ones. A count
# would not do -- the batches are the same size, so one read twice would hide
# one not read. A name with a newline is two lines; the key is on the second,
# and the key is what makes each line one file's.
: >"$work/expected"
planted=0
for name in "${names[@]}"; do
	if [ "$name" != scripts/check-no-secrets.sh ]; then
		printf -v key '%s-%s%06d' "$key_prefix" "$filler" "$planted"
		printf '%s\n' "$key" >"$big/$name"
		printf './%s:1:%s\n' "$name" "$key" >>"$work/expected"
		planted=$((planted + 1))
	fi
done
run_check "$big"
verdict every-file 1 "check-no-secrets: possible OpenAI-style key:"
{ search -F -e ":1:$key_prefix-" "$work/expected" || true; } | LC_ALL=C sort >"$work/expected.sorted"
{ search -F -e ":1:$key_prefix-" "$work/out" || true; } | LC_ALL=C sort >"$work/reported.sorted"
want_lines="$(wc -l <"$work/expected.sorted" | tr -d ' ')"
if [ "$want_lines" -ne "$planted" ]; then
	problem "every-file-set: $want_lines expected lines for $planted keys; this test is wrong"
elif ! cmp -s "$work/expected.sorted" "$work/reported.sorted"; then
	missing="$(LC_ALL=C comm -23 "$work/expected.sorted" "$work/reported.sorted" | wc -l | tr -d ' ')"
	extra="$(LC_ALL=C comm -13 "$work/expected.sorted" "$work/reported.sorted" | wc -l | tr -d ' ')"
	problem "every-file-set: of $planted keys, $missing not reported and $extra reported but not expected (or twice); one of each:"
	LC_ALL=C comm -23 "$work/expected.sorted" "$work/reported.sorted" | sed -n '1p' | LC_ALL=C cut -c 1-160 >&2
	LC_ALL=C comm -13 "$work/expected.sorted" "$work/reported.sorted" | sed -n '1p' | LC_ALL=C cut -c 1-160 >&2
else
	printf 'test-check-no-secrets: ok %-18s %d of %d keys, each once\n' every-file-set "$want_lines" "$planted"
	passed=$((passed + 1))
fi

# --- nothing to scan ---------------------------------------------------------

# A repository that tracks nothing, and one that tracks only a symlink, which
# the check does not read: each would pass having looked at nothing. The
# check's copy is untracked in both.
empty="$work/empty"
mkdir -p "$empty/scripts"
sg -C "$empty" init -q
cp "$check" "$empty/scripts/check-no-secrets.sh"
run_check "$empty"
verdict nothing-tracked 1 "check-no-secrets: no files to scan; the check would pass for the wrong reason"

links="$work/links"
mkdir -p "$links/scripts"
sg -C "$links" init -q
cp "$check" "$links/scripts/check-no-secrets.sh"
ln -s nowhere "$links/link"
sg -C "$links" add -f -- link
run_check "$links"
verdict only-a-symlink 1 "check-no-secrets: no files to scan; the check would pass for the wrong reason"

# --- a check that stops early ------------------------------------------------

# scripts/early-stop-env.sh writes a BASH_ENV file that stops the check: right
# after it installs its EXIT trap, by an unset variable under its `set -u`,
# `exit 7`, a `( exit 5 )` that `set -e` carries out, SIGTERM, SIGHUP or
# SIGINT sent to it alone, or SIGINT sent to it and a child it waits for
# (`stopped-*`); and by an unset variable just before its ok line, once every
# scratch file is made (`stopped-late`). Under bash 3.2 a stop by an unset
# variable left through the trap with status 0 and no output but bash's own
# line: a pass that scanned nothing. Each must exit with the status bash gives
# that stop, the stop must have fired with the check's EXIT trap installed,
# nothing may run past it, and the check's TMPDIR must be empty after it.
#
# And an arithmetic error, which bash does not stop on: it drops the command
# that holds it and goes on. One in the listing (`skipped-listing`, the
# second `read`) left the scan a list cut short, and one in the scan
# (`skipped-scan`, while it refuses a tracked .env) left it unfinished: each
# passed with the problem in the tree. Each must fail, by name.
early_stop_env="$repo_root/scripts/early-stop-env.sh"
small="$work/small"
mkdir -p "$small/scripts"
sg -C "$small" init -q
cp "$check" "$small/scripts/check-no-secrets.sh"
printf 'clean\n' >"$small/a.txt"
sg -C "$small" add -f -- a.txt scripts/check-no-secrets.sh
# A key in the last file git lists, and a tracked .env.
skip="$work/skip"
mkdir -p "$skip/scripts"
sg -C "$skip" init -q
cp "$check" "$skip/scripts/check-no-secrets.sh"
printf 'clean\n' >"$skip/a.txt"
printf 'clean\n' >"$skip/b.txt"
printf 'A=1\n' >"$skip/.env.zz-planted"
printf '%s\n' "$fake_key" >"$skip/zz-key.txt"
sg -C "$skip" add -f -- a.txt b.txt .env.zz-planted zz-key.txt scripts/check-no-secrets.sh
# <case>|<tree>|<stop>|<hook>|<status>|<text the output must hold, or ->
for row in "stopped-unset|$small|unset|trap-exit|1|-" "stopped-exit7|$small|exit7|trap-exit|7|-" \
	"stopped-subshell|$small|subshell|trap-exit|5|-" "stopped-term|$small|term|trap-exit|143|-" \
	"stopped-hup|$small|hup|trap-exit|129|-" "stopped-int|$small|int|trap-exit|130|-" \
	"stopped-int-group|$small|int-group|trap-exit|130|-" \
	"stopped-late|$small|unset|printf=check-no-secrets: ok*|1|-" \
	"skipped-listing|$skip|arith|read=2|1|check-no-secrets: the listing did not finish" \
	"skipped-scan|$skip|arith|printf=*zz-planted*|1|check-no-secrets: the scan did not finish"; do
	IFS='|' read -r name root stop hook want needle <<<"$row"
	"$early_stop_env" "$stop" "$hook" >"$work/$name.env"
	tmp="$work/tmp-$name"
	mkdir "$tmp"
	# Its stderr dropped: bash names the command a signal ended, paths and all.
	run_check "$root" "BASH_ENV=$work/$name.env" "TMPDIR=$tmp" 2>/dev/null
	bad=""
	if [ "$status" -ne "$want" ]; then
		bad="the check exited $status, expected $want"
	fi
	if ! search -q -x -F -e "early stop: $stop" "$work/out"; then
		bad="${bad:+$bad; }its output lacks 'early stop: $stop': the stop never fired"
	elif ! search -q -x -e '^early stop: trap -- .* EXIT$' "$work/out"; then
		bad="${bad:+$bad; }no EXIT trap was installed when it stopped"
	fi
	if [ "$needle" != - ] && ! search -q -F -e "$needle" "$work/out"; then
		bad="${bad:+$bad; }its output lacks '$needle'"
	fi
	if search -q -e '^check-no-secrets: ok' -e '^early stop: still running' "$work/out"; then
		bad="${bad:+$bad; }it ran on past the stop"
	fi
	left="$(find "$tmp" -mindepth 1 -print | wc -l | tr -d ' ')"
	if [ "$left" -ne 0 ]; then
		bad="${bad:+$bad; }it left $left file(s) in its TMPDIR"
	fi
	if [ -n "$bad" ]; then
		problem "$name: $bad; the first lines it printed:"
		LC_ALL=C sed -n '1,12p' "$work/out" | LC_ALL=C cut -c 1-200 >&2
	else
		printf 'test-check-no-secrets: ok %-18s exit %d\n' "$name" "$status"
		passed=$((passed + 1))
	fi
done

if [ "$failures" -ne 0 ]; then
	printf 'test-check-no-secrets: %d problem(s), %d case(s) passed\n' "$failures" "$passed" >&2
	exit 1
fi
printf 'test-check-no-secrets: ok (%d cases; %d tracked names past ARG_MAX %d)\n' "$passed" "$tracked" "$arg_max"
finished=1
