#!/usr/bin/env bash
#
# Proves that scripts/check-no-local-paths.sh refuses every shape it exists
# for, in every kind of file, and passes every form it must.
#
#   scripts/test-check-no-local-paths.sh
#
# Each case is a throwaway repository: the check copied under `scripts/`, the
# files the case plants beside it, all of them tracked. The check runs there
# as it runs in a checkout, and the case reads what it says. Every case's tree
# tracks the check itself, so a pattern that matched its own source would
# fail the case that must pass.
#
# Planted, and each required to be named by file and line:
#   - every shape: a macOS home, a Linux home, the harness scratch root as
#     macOS and Linux spell it, macOS's per-user temporary root with and
#     without `/private`, after `=`, `"`, a space, `file://`, an SGR
#     sequence (a coloured path in a capture) and a line start;
#   - in a file whose name git would quote (a tab and a byte past ASCII), one
#     named `-`, one under a directory named `-x`, one with no newline at its
#     end, and a binary file (refused by name: it cannot be marked);
#   - a placeholder and a real path on one line, and a marked line followed
#     by an unmarked one: an exception covers its own text, not its neighbour;
#   - a redirect into a home (`>` is no placeholder), a home with no slash
#     after it, a harness root broken after its `tmp/`, a mark in a request
#     body and in a `.txt` (no mark excuses a capture), and a mark that does
#     not end its line.
# Required to pass: the placeholders captures use, a relative path, the
# documentation's `<name>` and `<digits>` forms, `$TMPDIR` and `~`, each
# documented placeholder (bare too), a home inside a run (`<scratch>/home/...`),
# and a marked fixture -- counted as such.
# And the environment: `GIT_DIR` naming a clean decoy, `GIT_INDEX_FILE`
# naming an empty index, `GREP_OPTIONS`, and a HOME whose config sets
# `grep.column`, `color.grep` and an fsmonitor program must not change the
# answer or write to the decoy; a skip-worktree entry and a tracked file gone
# from the working tree are refused.
#
# Its paths are assembled at run time: this file is tracked and scanned.

set -euo pipefail
unset GREP_OPTIONS
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/xfx-local-paths-test.XXXXXX")"
trap 'rm -rf "$work"' EXIT

passed=0
failed=0
problem() {
	printf 'test-check-no-local-paths: FAIL %s\n' "$1" >&2
	failed=$((failed + 1))
}
ok() {
	printf 'test-check-no-local-paths: ok %-26s %s\n' "$1" "$2"
	passed=$((passed + 1))
}

scratch_git() {
	GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false "$@"
}

# A new tree at $1 holding the check, its own repository.
new_tree() {
	mkdir -p "$1/scripts"
	cp "$repo_root/scripts/check-no-local-paths.sh" "$1/scripts/"
	scratch_git -C "$1" init -q
}
track() {
	scratch_git -C "$1" add -A -f
}
# The check in tree $1, with the environment "${@:2}"; its exit status in
# `status`, its output in "$work/out".
run_check() {
	local tree="$1"
	shift
	status=0
	(cd "$tree" && env "$@" ./scripts/check-no-local-paths.sh) >"$work/out" 2>&1 </dev/null || status=$?
}
# Whether the last run named `<path>:<line>: <shape>` (or a binary file).
named() {
	grep -q -F -x -e "$1" "$work/out"
}
# How many hits the last run named.
hits() {
	grep -c -E '^[^ ].*:([0-9]+: | \(a binary file)' "$work/out" | tr -d ' ' || true
}
show() {
	sed 's/^/    | /' "$work/out" >&2
}

s='/'
mac="${s}Users${s}alice${s}"
linux="${s}home${s}bob${s}"
harness_mac="${s}private${s}tmp${s}claude-501"
harness_linux="${s}tmp${s}claude-1000"
folders="${s}private${s}var${s}folders${s}ab${s}T${s}"
folders_bare="${s}var${s}folders${s}ab${s}T${s}"
home_shape='a macOS home directory'
linux_shape='a Linux home directory'
harness_shape='a harness scratch root'
folders_shape='a macOS per-user temporary root'

# --- each shape, in each kind of file ----------------------------------------
t="$work/shapes"
new_tree "$t"
odd="$(printf 'caf\303\251\tname.md')"
printf 'cd %srepo\n' "$mac" >"$t/mac.md"
printf 'HOME=%s\n' "$linux" >"$t/linux.sh"
printf 'TMPDIR = "%s/x"\n' "$harness_mac" >"$t/harness.py"
printf 'one\n%s/x\n' "$harness_linux" >"$t/linux-harness.txt"
printf '\033]8;;file://%sshot.png\033\\x\033]8;;\033\\\n' "$folders" >"$t/link.ansi"
printf '{"cwd": "%sx"}\n' "$folders_bare" >"$t/body.json"
printf '\033[1m\033[38;5;245m%sx\033[39m\n' "$mac" >"$t/colour.ansi"
printf 'see %sx\n' "$mac" >"$t/$odd"
printf '%s\n' "$linux" >"$t/-"
mkdir -p "$t/-x"
printf 'a\nb\n%s' "$harness_mac" >"$t/-x/last.txt"
printf 'PNG\000%s\n' "$mac" >"$t/blob.bin"
printf '%s and %s\n' "${s}home${s}u${s}" "$linux" >"$t/both.rs"
printf '%s // local-path-ok\n%s\n' "$folders" "$folders" >"$t/marked.rs"
printf 'log >%sx\n' "$linux" >"$t/redirect.sh"
printf '<code>%sx</code>\n' "$mac" >"$t/tag.html"
printf 'cd %s\n(%s)\n' "${s}Users${s}alice" "${s}home${s}bob" >"$t/bare.md"
printf '  \033[38;5;250mclaude-501%sfxcap.Ab12Cd%swork\n' "$s" "$s" >"$t/wrapped.ansi"
mkdir -p "$t/fx-x-40x120-bodies"
printf '{"cwd":"%sx"} // local-path-ok\n' "$linux" >"$t/fx-x-40x120-bodies/001-completion.json"
printf '%sx # local-path-ok\n' "$mac" >"$t/notes.txt"
printf 'see %sx local-path-ok here\n' "$mac" >"$t/midline.md"
track "$t"
run_check "$t"
want=(
	"mac.md:1: $home_shape"
	"linux.sh:1: $linux_shape"
	"harness.py:1: $harness_shape"
	"linux-harness.txt:2: $harness_shape"
	"link.ansi:1: $folders_shape"
	"body.json:1: $folders_shape"
	"colour.ansi:1: $home_shape"
	"$odd:1: $home_shape"
	"-:1: $linux_shape"
	"-x/last.txt:3: $harness_shape"
	"blob.bin: (a binary file, which cannot be marked): a local path"
	"both.rs:1: $linux_shape"
	"marked.rs:2: $folders_shape"
	"redirect.sh:1: $linux_shape"
	"tag.html:1: $home_shape"
	"bare.md:1: $home_shape"
	"bare.md:2: $linux_shape"
	"wrapped.ansi:1: a harness scratch root, broken after its tmp/"
	"fx-x-40x120-bodies/001-completion.json:1: $linux_shape"
	"notes.txt:1: $home_shape"
	"midline.md:1: $home_shape"
)
missing=()
for line in "${want[@]}"; do
	named "$line" || missing+=("$line")
done
if [ "$status" -ne 1 ]; then
	problem "shapes: the check exited $status, not 1:"
	show
elif [ "${#missing[@]}" -ne 0 ]; then
	problem "shapes: not named: ${missing[*]}"
	show
elif [ "$(hits)" -ne "${#want[@]}" ]; then
	problem "shapes: $(hits) hits named, ${#want[@]} planted:"
	show
else
	ok shapes "${#want[@]} planted, each named by file and line"
fi

# --- what must pass -----------------------------------------------------------
# The check alone first: it passes on its own source, and its own marked and
# placeholder lines are the counts the next tree adds to.
t="$work/self"
new_tree "$t"
track "$t"
run_check "$t"
counts='^check-no-local-paths: ok \(([0-9]+) tracked file\(s\) scanned; ([0-9]+) marked fixture line\(s\), ([0-9]+) placeholder line\(s\)\)$'
self_marked=''
if [ "$status" -ne 0 ] || ! grep -q -E "$counts" "$work/out"; then
	problem "self: the check does not pass on a tree holding only itself (exit $status):"
	show
else
	self_marked="$(sed -n -E "s/$counts/\2/p" "$work/out")"
	self_placeheld="$(sed -n -E "s/$counts/\3/p" "$work/out")"
	ok self "its own source matches none of its shapes"
fi

t="$work/passes"
new_tree "$t"
{
	printf 'OSC 8: file://<work>/shot.png\n'
	printf 'mcp: <scratch>%shome%s.fx%smcp.json\n' "$s" "$s" "$s"
	printf 'lock: <tmp>%sbin%ssuite-lock.sh\n' "$s" "$s"
	printf 'old capture: fxcap.Ab12Cd%shome%s.xfx%sskills\n' "$s" "$s" "$s"
	printf 'relative: a%sb\n' "$mac"
	printf 'forms: %s<name>%s %s<name>%s %s<digits>\n' "${s}Users${s}" "$s" "${s}home${s}" "$s" "${s}private${s}tmp${s}claude-"
	printf 'derived: "$TMPDIR/x" "${HOME}/x" ~/x\n'
	printf 'placeholders: %s.zshrc %sa.png %sproject\n' "${s}home${s}u${s}" "${s}Users${s}me${s}" "${s}home${s}you${s}"
	printf 'fixture: %sx  # local-path-ok: a redaction input\n' "$folders"
	printf 'inside a run: <scratch>%shome%su%sx <work:real>%sUsers%scarol%snotes <tmp>%shome%sdave%s `%s` (%s)\n' \
		"$s" "$s" "$s" "$s" "$s" "$s" "$s" "$s" "$s" "${s}Users${s}me" "${s}home${s}me"
} >"$t/clean.md"
track "$t"
run_check "$t"
if [ "$status" -ne 0 ]; then
	problem "passes: the check exited $status on a tree with no local path:"
	show
elif [ -z "$self_marked" ] || ! grep -q -x -F "check-no-local-paths: ok (2 tracked file(s) scanned; $((self_marked + 1)) marked fixture line(s), $((self_placeheld + 2)) placeholder line(s))" "$work/out"; then
	problem "passes: the check passed without counting the one marked line and the two placeholder lines:"
	show
else
	ok passes "placeholders, relative and derived paths, one marked fixture"
fi

# --- the environment ----------------------------------------------------------
t="$work/env"
new_tree "$t"
printf 'x\n%s/a\n' "$harness_mac" >"$t/plant.md"
track "$t"
decoy="$work/decoy"
mkdir -p "$decoy"
scratch_git -C "$decoy" init -q
printf 'clean\n' >"$decoy/a.md"
scratch_git -C "$decoy" add a.md
scratch_git -C "$decoy" -c user.name=t -c user.email=t@example.invalid commit -q -m decoy
: >"$work/empty.index"
home="$work/home"
mkdir -p "$home"
ran="$work/fsmonitor-ran"
printf '#!/bin/sh\n: >"%s"\n' "$ran" >"$home/fsmonitor"
chmod 700 "$home/fsmonitor"
printf '[grep]\n\tcolumn = true\n\tlineNumber = false\n\tfullName = true\n[color]\n\tgrep = always\n[core]\n\tfsmonitor = %s\n' "$home/fsmonitor" >"$home/.gitconfig"
decoy_state() {
	(cd "$decoy/.git" && find . -type f -print | LC_ALL=C sort | while IFS= read -r f; do
		printf '%s %s\n' "$(cksum <"$f")" "$f"
	done)
}
before="$(decoy_state)"
for case in "GIT_DIR=$decoy/.git" "GIT_INDEX_FILE=$work/empty.index" "GIT_WORK_TREE=$decoy" "GREP_OPTIONS=-v" "HOME=$home"; do
	run_check "$t" "$case"
	label="env ${case%%=*}"
	if [ "$status" -ne 1 ] || ! named "plant.md:2: $harness_shape" || [ "$(hits)" -ne 1 ]; then
		problem "$label: the plant was not named, alone, by file and line (exit $status):"
		show
	elif [ "$(decoy_state)" != "$before" ]; then
		problem "$label: the check wrote into the decoy repository"
	elif [ -e "$ran" ]; then
		problem "$label: a program from config ran"
	else
		ok "$label" "the plant named, the decoy unchanged"
	fi
done

# --- entries git grep would pass over -----------------------------------------
t="$work/skipped"
new_tree "$t"
printf '%s\n' "$mac" >"$t/hidden.md"
track "$t"
scratch_git -C "$t" update-index --skip-worktree hidden.md
run_check "$t"
if [ "$status" -ne 1 ] || ! grep -q 'skip-worktree or assume-unchanged' "$work/out" || ! named hidden.md; then
	problem "skip-worktree: a skip-worktree entry was not refused by name (exit $status):"
	show
else
	ok skip-worktree "refused by name"
fi

t="$work/deleted"
new_tree "$t"
printf '%s\n' "$mac" >"$t/gone.md"
track "$t"
rm "$t/gone.md"
run_check "$t"
if [ "$status" -ne 1 ] || ! grep -q 'missing from the working tree' "$work/out" || ! named gone.md; then
	problem "deleted: a tracked file gone from the working tree was not refused by name (exit $status):"
	show
else
	ok deleted "refused by name"
fi

if [ "$failed" -ne 0 ]; then
	printf 'test-check-no-local-paths: %d of %d case(s) failed\n' "$failed" "$((passed + failed))" >&2
	exit 1
fi
printf 'test-check-no-local-paths: ok (%d cases)\n' "$passed"
