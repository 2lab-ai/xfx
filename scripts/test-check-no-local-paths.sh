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
#     end, and two binary files (refused by name: they cannot be marked),
#     one with its NUL past git's first 8000 bytes and before the path on
#     the same line, which no shell variable can hold;
#   - a placeholder and a real path on one line, and a marked line followed
#     by an unmarked one: an exception covers its own text, not its neighbour;
#   - a redirect into a home (`>` is no placeholder), a home with no slash
#     after it, a harness root broken after its `tmp/` (at a row's start
#     and after a backtick, as a README quotes it; and two known
#     conservative matches, after `@` and after a letter past ASCII), a mark
#     in a request
#     body, in a `.txt` and in a `/trace` report's `.md` (a capture by its
#     name), a mark that does not end its line, and a home whose name starts
#     with a documented placeholder's on a line that ends in that placeholder;
#   - capture.sh's reserved names, both ways: a report renamed out of them
#     (`trace-report.md`, an unmarked line: refused as any authored file's)
#     and an authored file named into them (`fx-guide-40x120-notes.md`, a
#     marked line: a capture, which no mark excuses).
# Required to pass: the placeholders captures use, a relative path, the
# documentation's `<name>` and `<digits>` forms, `$TMPDIR` and `~`, a model
# path with `claude-<digits>/` inside a name or after a `/`, each
# documented placeholder (bare too), a home inside a run (`<scratch>/home/...`),
# a marked fixture -- counted as such -- and a report renamed out of
# capture.sh's reserved names with its line marked, read as authored text
# (counted as marked too: the convention's limit, which the name is for).
# And the environment: `GIT_DIR` naming a clean decoy, `GIT_INDEX_FILE`
# naming an empty index, `GREP_OPTIONS`, and a HOME whose config sets
# `grep.column`, `color.grep` and an fsmonitor program must not change the
# answer or write to the decoy; a skip-worktree entry and a tracked file gone
# from the working tree are refused, and so is one that goes while the scan
# runs. And a `git` that gives a line in which the check's own reading finds
# no shape and no placeholder: the check refuses that line rather than
# counting it as one that passed (a `git` on PATH in front of the real one
# makes both happen on demand).
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
{
	printf '%8100s' '' | tr ' ' a
	printf '\000 cd %sx\n' "$linux"
} >"$t/late-nul.md"
printf '%s and %s\n' "${s}home${s}u${s}" "$linux" >"$t/both.rs"
printf '%s // local-path-ok\n%s\n' "$folders" "$folders" >"$t/marked.rs"
printf 'log >%sx\n' "$linux" >"$t/redirect.sh"
printf '<code>%sx</code>\n' "$mac" >"$t/tag.html"
printf 'cd %s\n(%s)\n' "${s}Users${s}alice" "${s}home${s}bob" >"$t/bare.md"
printf '  \033[38;5;250mclaude-501%sfxcap.Ab12Cd%swork\n' "$s" "$s" >"$t/wrapped.ansi"
printf 'a short directory in the harness temp folder (`claude-%s%sp5uc`)\n' 501 "$s" >"$t/ticked.md"
# Known conservative matches, refused on purpose: `@` and a letter past ASCII
# can be part of a name, but the shape's left context is a heuristic (a
# character no ASCII path name of these repositories holds), so these fail
# toward refusal.
printf 'cache@claude-3%sx\n' "$s" >"$t/at-sign.md"
printf '\303\251claude-3%sx\n' "$s" >"$t/accented.md"
mkdir -p "$t/fx-x-40x120-bodies"
printf '{"cwd":"%sx"} // local-path-ok\n' "$linux" >"$t/fx-x-40x120-bodies/001-completion.json"
printf '%sx # local-path-ok\n' "$mac" >"$t/notes.txt"
printf 'see %sx local-path-ok here\n' "$mac" >"$t/midline.md"
mkdir -p "$t/w3"
printf 'Workspace: %swork  # local-path-ok\n' "$linux" >"$t/w3/xfx-trace-40x120-report-1.md"
printf 'cp x %subuntu%sy; cd %su\n' "${s}home${s}" "$s" "${s}home${s}" >"$t/sentinel.sh"
printf 'Workspace: %swork\n' "$linux" >"$t/w3/trace-report.md"
mkdir -p "$t/docs"
printf 'cd %swork  # local-path-ok: an example\n' "$linux" >"$t/docs/fx-guide-40x120-notes.md"
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
	"late-nul.md: (a binary file, which cannot be marked): a local path"
	"both.rs:1: $linux_shape"
	"marked.rs:2: $folders_shape"
	"redirect.sh:1: $linux_shape"
	"tag.html:1: $home_shape"
	"bare.md:1: $home_shape"
	"bare.md:2: $linux_shape"
	"wrapped.ansi:1: a harness scratch root, broken after its tmp/"
	"ticked.md:1: a harness scratch root, broken after its tmp/"
	"at-sign.md:1: a harness scratch root, broken after its tmp/"
	"accented.md:1: a harness scratch root, broken after its tmp/"
	"fx-x-40x120-bodies/001-completion.json:1: $linux_shape"
	"notes.txt:1: $home_shape"
	"midline.md:1: $home_shape"
	"w3/xfx-trace-40x120-report-1.md:1: $linux_shape"
	"sentinel.sh:1: $linux_shape"
	"w3/trace-report.md:1: $linux_shape"
	"docs/fx-guide-40x120-notes.md:1: $linux_shape"
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
	printf 'a model path: models%sclaude-3%sx, anthropic.claude-3%sx, my-claude-2%sx\n' "$s" "$s" "$s" "$s"
	printf 'forms: %s<name>%s %s<name>%s %s<digits>\n' "${s}Users${s}" "$s" "${s}home${s}" "$s" "${s}private${s}tmp${s}claude-"
	printf 'derived: "$TMPDIR/x" "${HOME}/x" ~/x\n'
	printf 'placeholders: %s.zshrc %sa.png %sproject\n' "${s}home${s}u${s}" "${s}Users${s}me${s}" "${s}home${s}you${s}"
	printf 'fixture: %sx  # local-path-ok: a redaction input\n' "$folders"
	printf 'inside a run: <scratch____>%shome%su%sx <work:real__>%sUsers%scarol%snotes <h__>%shome%sdave%s `%s` (%s) cd %s\n' \
		"$s" "$s" "$s" "$s" "$s" "$s" "$s" "$s" "$s" "${s}Users${s}me" "${s}home${s}me" "${s}home${s}u"
} >"$t/clean.md"
mkdir -p "$t/w3"
printf 'Workspace: %swork  # local-path-ok\n' "$linux" >"$t/w3/trace-report.md"
track "$t"
run_check "$t"
if [ "$status" -ne 0 ]; then
	problem "passes: the check exited $status on a tree with no local path:"
	show
elif [ -z "$self_marked" ] || ! grep -q -x -F "check-no-local-paths: ok (3 tracked file(s) scanned; $((self_marked + 2)) marked fixture line(s), $((self_placeheld + 2)) placeholder line(s))" "$work/out"; then
	problem "passes: the check passed without counting the two marked lines and the two placeholder lines:"
	show
else
	ok passes "placeholders, relative and derived paths, a marked fixture, a renamed report"
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

# --- what a scan cannot vouch for, made to happen ------------------------------
# A `git` in front of the real one on PATH, which the check runs as `git`:
# it passes everything to the real one and, by SHIM_MODE, changes one thing.
shim="$work/shim"
mkdir -p "$shim"
cat >"$shim/git" <<'SH'
#!/usr/bin/env bash
case "$SHIM_MODE: $* " in
# The tree's scan (`-C .`, not the control's) gives one line more, of a file
# with no shape in it.
disagree:*' -C . '*' grep '*)
	"$SHIM_REAL" "$@"
	s=$?
	if [ "$s" -gt 1 ]; then exit "$s"; fi
	printf 'clean.md\000%s\000%s\n' 1 'nothing here'
	exit 0
	;;
# The plant goes as the tree's scan starts, before git reads it.
vanish:*' -C . '*' grep '*)
	rm -f vanish.md
	;;
esac
exec "$SHIM_REAL" "$@"
SH
chmod 700 "$shim/git"
shim_env=("PATH=$shim:$PATH" "SHIM_REAL=$(command -v git)")

t="$work/disagree"
new_tree "$t"
printf 'nothing here\n' >"$t/clean.md"
track "$t"
run_check "$t" "${shim_env[@]}" SHIM_MODE=disagree
if [ "$status" -ne 1 ] || ! named "clean.md:1: a line git matched in which this shell finds no shape and no placeholder" || [ "$(hits)" -ne 1 ]; then
	problem "disagree: a line git gave that the check could not account for was not refused (exit $status):"
	show
else
	ok disagree "refused, not counted as passing"
fi

t="$work/vanished"
new_tree "$t"
printf '%s\n' "$mac" >"$t/vanish.md"
track "$t"
run_check "$t" "${shim_env[@]}" SHIM_MODE=vanish
if [ "$status" -ne 1 ] || [ -e "$t/vanish.md" ] || ! grep -q 'missing from the working tree' "$work/out" || ! named vanish.md; then
	problem "vanished: a tracked file gone while the scan ran was not refused by name (exit $status):"
	show
else
	ok vanished "refused by name"
fi

if [ "$failed" -ne 0 ]; then
	printf 'test-check-no-local-paths: %d of %d case(s) failed\n' "$failed" "$((passed + failed))" >&2
	exit 1
fi
printf 'test-check-no-local-paths: ok (%d cases)\n' "$passed"
