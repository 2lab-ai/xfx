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
#     marked line: a capture, which no mark excuses);
#   - tracked symlinks whose targets are homes, one under a directory and
#     pointing at one (git grep does not read a symlink).
# And the check must print the accounting this tree gives (pinned from its
# plants and the `self` case's counts). When `shapes` fails it says, for
# each plant not named, what the failing run's own git pass gave of it (the
# check's trace: line numbers and classes, never text) and what the tree
# holds after it, and runs the check again on the same tree: per tree or per
# run. It prints versions and config keys by scope, never a path: the
# `diagnosis` case runs it under a global config whose origin and value are
# paths.
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
# counting it as one that passed; a line with no number, a stream that ends
# inside a record, an index listing cut inside its last entry (the symlink
# listing and the tracked one), a `git grep` that fails without a word, and a
# line pass that leaves out a file the count pass has, or gives one line of a
# file fewer (the shape a CI run showed), stop the scan, saying
# whether the disagreement repeated on a second look (the check fails
# either way), or that the second look could not read the file when its
# greps fail or give fields that are not numbers; and each grep of the
# tree, the second look's too, runs on one thread (a `git`
# on PATH in front of the real one makes each happen on demand). And a check
# stopped by an error of its own (a BASH_ENV function under `set -u`) fails.
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
# Under bash 3.2 a script that stops on an error of its own (an unset
# variable under `set -u`) leaves through an EXIT trap with status 0, so a
# run that never reached its end would read as a pass: the trap fails any
# exit before `finished=1`.
finished=0
trap 'rm -rf "$work"; [ "$finished" = 1 ] || exit 1' EXIT

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
	grep -c -E '^[^ ].*:([0-9]+: | \((a binary file|a symlink))' "$work/out" | tr -d ' ' || true
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

# --- the check alone ----------------------------------------------------------
# It passes on its own source, and its own marked and placeholder lines are
# the counts the trees below add to.
t="$work/self"
new_tree "$t"
track "$t"
run_check "$t"
counts='^check-no-local-paths: ok \(([0-9]+) tracked file\(s\) scanned; ([0-9]+) marked fixture line\(s\), ([0-9]+) placeholder line\(s\); ([0-9]+) symlink\(s\)\)$'
self_marked=''
if [ "$status" -ne 0 ] || ! grep -q -E "$counts" "$work/out"; then
	problem "self: the check does not pass on a tree holding only itself (exit $status):"
	show
else
	self_marked="$(sed -n -E "s/$counts/\2/p" "$work/out")"
	self_placeheld="$(sed -n -E "s/$counts/\3/p" "$work/out")"
	ok self "its own source matches none of its shapes"
fi

# On a `shapes` failure: what ran, the accounting it should have printed,
# and for each plant not named, first what the failing run's own git pass
# gave of it (its trace: line numbers and classes, never text), then what
# the tree holds after it. A plant in the index and on disk, which a later
# grep matches and of which the failing pass gave no line, is one the check's
# own git pass left out. A second run over the same tree then says whether
# that happens per tree or per run.
diagnose_shapes() {
	local line plant gave class number path indexed listed ls_status bytes probe probe_status scope key value again=0 refused_only=0
	# Versions only, and only the config keys that change a scan, each with
	# its scope and a value only when it is a boolean or a number: this log
	# is public, and a path (a tool's, a config file's, a value's) names a
	# machine. Its lines go to fd 3 and nothing else does: every command
	# below drops its own stderr (a `cd` that fails names the directory),
	# and the caller drops fd 2 as well.
	printf '    | ran: bash %s; %s\n' "$BASH_VERSION" "$(git --version 2>/dev/null || echo 'git: no version')" >&3
	# Names only, never a value as text (a value can span lines, and its
	# second line would read as a name): each name with its scope, both
	# checked against what git can print (`<odd>` otherwise), and the value
	# in effect only as git types it, a boolean or a number, `<set>` if not.
	{ (cd "$t" && git config --show-scope --name-only --get-regexp '^(grep\.|core\.fsmonitor|index\.|feature\.)') 2>/dev/null || true; } |
		while IFS= read -r line; do
			scope="${line%%$'\t'*}" key="${line#*$'\t'}"
			case "$scope" in
			system | global | local | worktree | command) ;;
			*) scope='<odd>' ;;
			esac
			case "$key" in
			'' | *[!A-Za-z0-9.-]*) key='<odd>' value='<set>' ;;
			*) value="$( (cd "$t" && git config --type=bool-or-int --get "$key") 2>/dev/null)" || value='<set>' ;;
			esac
			case "$value" in
			true | false | [0-9]* | -[0-9]*) ;;
			*) value='<set>' ;;
			esac
			case "$value" in
			*[!0-9a-z-]*) value='<set>' ;;
			esac
			printf '    | config: %s %s %s\n' "$scope" "$key" "$value"
		done >&3
	printf '    | expected: %s\n    | actual:   %s\n' "$accounting" "$(grep -E '^check-no-local-paths: git gave ' "$work/out" 2>/dev/null || echo none)" >&3
	for line in ${missing[@]+"${missing[@]}"}; do
		plant="${line%%: *}"
		plant="${plant%:[0-9]*}"
		gave=""
		if [ ! -f "$work/trace" ]; then
			gave="no trace written"
		else
			{
				while IFS= read -r -d '' class && IFS= read -r -d '' number && IFS= read -r -d '' path; do
					if [ "$path" = "$plant" ]; then
						gave="${gave:+$gave, }line $number $class"
					fi
				done <"$work/trace"
			} 2>/dev/null || true
			# Refused in the failing pass and not printed: the scan failed
			# after it (its own message says why), so this plant says nothing
			# more.
			case "$gave" in
			'' | *marked* | *placeholder*) ;;
			*)
				refused_only=$((refused_only + 1))
				continue
				;;
			esac
		fi
		ls_status=0
		listed="$(scratch_git -C "$t" ls-files -- ":(literal)$plant" 2>/dev/null)" || ls_status=$?
		indexed=0
		[ -z "$listed" ] || indexed="$(printf '%s\n' "$listed" | wc -l | tr -d ' ')"
		if [ -L "$t/$plant" ]; then
			bytes="a symlink"
		else
			bytes="$({ wc -c <"$t/$plant"; } 2>/dev/null | tr -d ' ' || true)"
			bytes="${bytes:+$bytes byte(s)}"
		fi
		probe_status=0
		probe="$( (cd "$t" && git -c core.fsmonitor=false -c grep.column=false -c grep.fullName=false -c color.grep=never \
			-c submodule.recurse=false --work-tree=. grep -a --no-color -c -E -e '/hom[e]/' -e '/User[s]/' \
			-e 'claud[e]-[0-9]' -e '/var/folder[s]/' -- ":(literal)$plant") 2>/dev/null)" || probe_status=$?
		printf '    | lost: %s: in the failing pass: %s; after it: in the index %s time(s) (ls-files exit %s), %s on disk, a later git grep -c: %s (exit %s)\n' \
			"$plant" "${gave:-no line of it}" "$indexed" "$ls_status" "${bytes:-no file}" "$(printf '%s' "${probe:-no line}" | tr '\n' ' ')" "$probe_status" >&3
	done
	[ "$refused_only" -eq 0 ] ||
		printf '    | %s more not named were refused in the failing pass (its trace), before the scan failed\n' "$refused_only" >&3
	cp "$work/out" "$work/out.first" 2>/dev/null || true
	run_check "$t" 2>/dev/null
	for line in ${missing[@]+"${missing[@]}"}; do
		named "$line" 2>/dev/null || again=$((again + 1))
	done
	if [ "${#missing[@]}" -ne 0 ] && [ "$again" -eq 0 ]; then
		printf '    | per run: a second run over the same tree (exit %s) named every one\n' "$status" >&3
	elif [ "${#missing[@]}" -ne 0 ]; then
		printf '    | per tree: a second run over the same tree (exit %s) missed %s of them too\n' "$status" "$again" >&3
	fi
	printf '    | second run: %s named; %s\n' "$(hits 2>/dev/null)" "$(grep -E '^check-no-local-paths: git gave ' "$work/out" 2>/dev/null || echo 'no accounting')" >&3
	cp "$work/out.first" "$work/out" 2>/dev/null || true
}

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
ln -s "${linux}x" "$t/home-link"
mkdir -p "$t/sub"
ln -s "$mac" "$t/sub/dir-link"
track "$t"
rm -f "$work/trace"
run_check "$t" "CHECK_NO_LOCAL_PATHS_TRACE=$work/trace"
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
	"home-link: (a symlink, whose target is published): $linux_shape"
	"sub/dir-link: (a symlink, whose target is published): $home_shape"
)
# What the check's git pass gives for this tree: a line for each plant above
# but the two symlinks (git grep does not read one), two of them in the
# binary files; marked.rs:1, a mark in authored text; and the check's own
# marked and placeholder lines (`self`).
lines_planted=$((${#want[@]} - 2))
accounting="check-no-local-paths: git gave $((lines_planted + 1 + ${self_marked:-0} + ${self_placeheld:-0})) line(s): $((lines_planted - 2)) refused, 2 in 2 binary file(s), $((1 + ${self_marked:-0})) marked, ${self_placeheld:-0} placeholder"
missing=()
for line in "${want[@]}"; do
	named "$line" || missing+=("$line")
done
if [ "$status" -ne 1 ] || [ "${#missing[@]}" -ne 0 ] || [ "$(hits)" -ne "${#want[@]}" ] || ! grep -q -x -F "$accounting" "$work/out"; then
	problem "shapes: exit $status; $(hits) of ${#want[@]} named; not named: ${missing[*]:-none}"
	show
	diagnose_shapes 3>&2 2>/dev/null
else
	ok shapes "${#want[@]} planted, each named by file and line"
fi

# --- what must pass -----------------------------------------------------------
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
elif [ -z "$self_marked" ] || ! grep -q -x -F "check-no-local-paths: ok (3 tracked file(s) scanned; $((self_marked + 2)) marked fixture line(s), $((self_placeheld + 2)) placeholder line(s); 0 symlink(s))" "$work/out"; then
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
# with no shape in it, in both its passes.
disagree:*' -C . '*' grep '*' -n '*)
	"$SHIM_REAL" "$@"
	s=$?
	if [ "$s" -gt 1 ]; then exit "$s"; fi
	printf 'clean.md\000%s\000%s\n' 1 'nothing here'
	exit 0
	;;
disagree:*' -C . '*' grep '*' -c '*)
	"$SHIM_REAL" "$@"
	s=$?
	if [ "$s" -gt 1 ]; then exit "$s"; fi
	printf 'clean.md\000%s\n' 1
	exit 0
	;;
# The plant goes as the tree's scan starts, before git reads it.
vanish:*' -C . '*' grep '*)
	rm -f vanish.md
	;;
# The tree's scan gives a line with no number.
badnum:*' -C . '*' grep '*' -n '*)
	"$SHIM_REAL" "$@"
	s=$?
	if [ "$s" -gt 1 ]; then exit "$s"; fi
	printf 'clean.md\000%s\000%s\n' x 'nothing here'
	exit 0
	;;
# The tree's line pass gives nothing, as if it found no line: every time,
# or only the first time (the second look then finds the line).
omit:*' -C . '*' grep '*' -n '*)
	exit 1
	;;
# The tree's line pass gives one line of a file fewer than its count does,
# the shape CI showed: plant.md's second line is dropped from it.
fewer:*' -C . '*' grep '*' -n '*)
	"$SHIM_REAL" "$@" >"$SHIM_TMP/fewer.out"
	s=$?
	if [ "$s" -gt 1 ]; then exit "$s"; fi
	while IFS= read -r -d '' p && IFS= read -r -d '' n && IFS= read -r text; do
		[ "$p:$n" = plant.md:2 ] || printf '%s\0%s\0%s\n' "$p" "$n" "$text"
	done <"$SHIM_TMP/fewer.out"
	exit "$s"
	;;
omitonce:*' -C . '*' grep '*' -n '*)
	if [ ! -e "$SHIM_TMP/omitted" ]; then
		: >"$SHIM_TMP/omitted"
		exit 1
	fi
	;;
# A grep of the tree on more than one thread is refused, so the check passes
# only if each of its passes runs on one.
threads:*' -C . '*' grep '*)
	case " $* " in
	*' grep.threads=1 '*) ;;
	*)
		echo 'git grep of the tree on more than one thread' >&2
		exit 2
		;;
	esac
	# And the tree's first line pass leaves the file out, so the second
	# look's greps run too, under the same rule.
	case " $* " in
	*' -n '*' -- ') exit 1 ;;
	esac
	;;
# The first line pass leaves the file out; then the second look's greps
# give nothing and fail (`lookfail`), or one gives a field that is not a
# number: the line number (`lookjunk`) or the count (`lookjunkc`).
lookfail:*' -C . '*' grep '*' -n '* | lookjunk:*' -C . '*' grep '*' -n '*' -- ' | lookjunkc:*' -C . '*' grep '*' -n '*' -- ')
	exit 1
	;;
lookjunkc:*' -C . '*' grep '*' -c '*':(literal)'*)
	printf 'plant.md\000%s\n' 2JUNK-COUNT
	exit 0
	;;
lookfail:*' -C . '*' grep '*' -n '*':(literal)'*)
	exit 1
	;;
lookfail:*' -C . '*' grep '*' -c '*':(literal)'*)
	exit 128
	;;
lookjunk:*' -C . '*' grep '*' -n '*':(literal)'*)
	printf 'plant.md\000%s\000%s\n' JUNK-NUMBER 'text'
	exit 0
	;;

# The tree's line pass ends inside a record.
truncated:*' -C . '*' grep '*' -n '*)
	"$SHIM_REAL" "$@"
	s=$?
	if [ "$s" -gt 1 ]; then exit "$s"; fi
	printf 'clean.md'
	exit 0
	;;
# A listing of the tree's index that loses its last byte, the NUL that ends
# its last entry: the symlink listing (`-s`) or the tracked one (`-v`).
cutlinks:*' -C . '*' ls-files -s '* | cuttracked:*' -C . '*' ls-files -v '*)
	"$SHIM_REAL" "$@" >"$SHIM_TMP/listing"
	s=$?
	n="$(wc -c <"$SHIM_TMP/listing")"
	head -c "$((n - 1))" "$SHIM_TMP/listing"
	exit "$s"
	;;
# The tree's scan fails, saying nothing.
gitfail:*' -C . '*' grep '*' -n '*)
	exit 2
	;;
esac
exec "$SHIM_REAL" "$@"
SH
chmod 700 "$shim/git"
shim_env=("PATH=$shim:$PATH" "SHIM_REAL=$(command -v git)" "SHIM_TMP=$work")

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

t="$work/badnum"
new_tree "$t"
printf 'nothing here\n' >"$t/clean.md"
track "$t"
run_check "$t" "${shim_env[@]}" SHIM_MODE=badnum
if [ "$status" -ne 1 ] || ! grep -q 'gave a line of clean.md with no line number' "$work/out" || ! grep -q 'the scan could not finish' "$work/out"; then
	problem "badnum: a line git gave with no line number did not stop the scan (exit $status):"
	show
else
	ok badnum "the scan stops"
fi

t="$work/gitfail"
new_tree "$t"
printf '%s\n' "$mac" >"$t/plant.md"
track "$t"
run_check "$t" "${shim_env[@]}" SHIM_MODE=gitfail
if [ "$status" -ne 1 ] || ! grep -q 'git grep could not search . (status 2)' "$work/out" || ! grep -q 'the scan could not finish' "$work/out"; then
	problem "gitfail: a git grep that failed did not fail the check (exit $status):"
	show
else
	ok gitfail "the check fails"
fi

t="$work/omit"
new_tree "$t"
printf '%s\n' "$mac" >"$t/plant.md"
track "$t"
run_check "$t" "${shim_env[@]}" SHIM_MODE=omit
if [ "$status" -ne 1 ] || ! grep -q 'two readings of the tree disagree: the line pass has (nothing) with 0 line(s), git grep -c has plant.md with 1' "$work/out" ||
	! grep -q -x -F 'check-no-local-paths: a second look at plant.md: the line pass gave line(s) none the first time and none now; git grep -c gives 1 now: the disagreement repeated on a second look' "$work/out" ||
	! grep -q 'the scan could not finish' "$work/out"; then
	problem "omit: a line pass that left out a file the count pass has did not fail the check (exit $status):"
	show
else
	ok omit "the two readings disagree; the check fails"
fi

# The line pass gives one line of a file fewer than the count (CI's shape):
# the check fails, naming the file and both counts, and the second look says
# it repeated.
t="$work/fewer"
new_tree "$t"
printf '%s\n%s\n' "$mac" "$linux" >"$t/plant.md"
track "$t"
run_check "$t" "${shim_env[@]}" SHIM_MODE=fewer
if [ "$status" -ne 1 ] || ! grep -q -x -F 'check-no-local-paths: two readings of the tree disagree: the line pass has plant.md with 1 line(s), git grep -c has plant.md with 2' "$work/out" ||
	! grep -q -x -F 'check-no-local-paths: a second look at plant.md: the line pass gave line(s) 1 the first time and 1 now; git grep -c gives 2 now: the disagreement repeated on a second look' "$work/out" ||
	! grep -q 'the scan could not finish' "$work/out"; then
	problem "fewer: a line pass with one line of a file fewer than the count did not fail the check, naming both (exit $status):"
	show
else
	ok fewer "the two readings disagree; the check fails"
fi

# The line pass leaves the file out once: the second look finds it, and the
# check still fails, having said the disagreement did not repeat.
t="$work/omitonce"
new_tree "$t"
printf '%s\n' "$mac" >"$t/plant.md"
track "$t"
rm -f "$work/omitted"
run_check "$t" "${shim_env[@]}" SHIM_MODE=omitonce
if [ "$status" -ne 1 ] || [ ! -e "$work/omitted" ] ||
	! grep -q -x -F 'check-no-local-paths: a second look at plant.md: the line pass gave line(s) none the first time and 1 now; git grep -c gives 1 now: the disagreement did not repeat on a second look' "$work/out" ||
	! grep -q 'the scan could not finish' "$work/out" || grep -q '^check-no-local-paths: ok' "$work/out"; then
	problem "omitonce: a line pass that left out a file once did not fail the check, or the second look did not say it did not repeat (exit $status):"
	show
else
	ok omitonce "not repeated, and the check fails"
fi

# Each grep of the tree runs on one thread (the shim refuses any other), the
# second look's too: the first line pass leaves the file out, and the second
# look must read it.
t="$work/threads"
new_tree "$t"
printf '%s\n' "$mac" >"$t/plant.md"
track "$t"
run_check "$t" "${shim_env[@]}" SHIM_MODE=threads
if [ "$status" -ne 1 ] || grep -q 'more than one thread' "$work/out" ||
	! grep -q -x -F 'check-no-local-paths: a second look at plant.md: the line pass gave line(s) none the first time and 1 now; git grep -c gives 1 now: the disagreement did not repeat on a second look' "$work/out"; then
	problem "threads: a grep of the tree ran on more than one thread (exit $status):"
	show
else
	ok threads "every grep on one thread, the second look's too"
fi

# The second look's greps fail, or give fields that are not numbers: it says
# it could not read the file and prints no field, and the check still fails.
for mode in lookfail lookjunk lookjunkc; do
	t="$work/$mode"
	new_tree "$t"
	printf '%s\n' "$mac" >"$t/plant.md"
	track "$t"
	run_check "$t" "${shim_env[@]}" SHIM_MODE=$mode
	case "$mode" in
	lookfail) want_look='check-no-local-paths: a second look at plant.md could not read the file (git exit 1 / 128)' ;;
	lookjunk | lookjunkc) want_look='check-no-local-paths: a second look at plant.md could not read the file (git exit 0 / 0)' ;;
	esac
	if [ "$status" -ne 1 ] || ! grep -q -x -F "$want_look" "$work/out" || grep -q -e 'JUNK' -e 'did not repeat' "$work/out" ||
		! grep -q 'the scan could not finish' "$work/out"; then
		problem "$mode: a second look that could not read the file read as a reading (exit $status):"
		show
	else
		ok "$mode" "could not read it, no field printed"
	fi
done

t="$work/truncated"
new_tree "$t"
printf 'nothing here\n' >"$t/clean.md"
track "$t"
run_check "$t" "${shim_env[@]}" SHIM_MODE=truncated
if [ "$status" -ne 1 ] || ! grep -q 'git grep ended inside a record of clean.md' "$work/out" || ! grep -q 'the scan could not finish' "$work/out"; then
	problem "truncated: a stream that ended inside a record did not stop the scan (exit $status):"
	show
else
	ok truncated "the scan stops"
fi

# A symlink to a home, the index's last entry, in a listing cut before its
# last NUL; and a skip-worktree entry, last, in the tracked listing cut so.
t="$work/cutlinks"
new_tree "$t"
ln -s "${mac}x" "$t/zz-link"
track "$t"
run_check "$t" "${shim_env[@]}" SHIM_MODE=cutlinks
if [ "$status" -ne 1 ] || ! grep -q 'git ls-files ended inside an entry of zz-link' "$work/out" || grep -q '^check-no-local-paths: ok' "$work/out"; then
	problem "cutlinks: a symlink listing cut inside its last entry did not stop the scan (exit $status):"
	show
else
	ok cutlinks "the scan stops"
fi
t="$work/cuttracked"
new_tree "$t"
printf '%s\n' "$mac" >"$t/zz-hidden.md"
track "$t"
scratch_git -C "$t" update-index --skip-worktree zz-hidden.md
run_check "$t" "${shim_env[@]}" SHIM_MODE=cuttracked
if [ "$status" -ne 1 ] || ! grep -q 'git ls-files ended inside an entry of zz-hidden.md' "$work/out" || grep -q '^check-no-local-paths: ok' "$work/out"; then
	problem "cuttracked: a tracked listing cut inside its last entry did not stop the check (exit $status):"
	show
else
	ok cuttracked "the check stops"
fi

# The check stopped by an error of its own: a `cmp` from BASH_ENV that reads
# an unset variable, under the check's `set -u`. Under bash 3.2 that exit
# goes through the EXIT trap with status 0 unless the trap fails it.
t="$work/stopped"
new_tree "$t"
track "$t"
printf 'cmp() { : "$%s"; }\n' no_such_var >"$work/stop.env"
run_check "$t" "BASH_ENV=$work/stop.env"
if [ "$status" -ne 1 ] || grep -q '^check-no-local-paths: ok' "$work/out"; then
	problem "stopped: a check stopped by its own error did not fail (exit $status):"
	show
else
	ok stopped "the check fails"
fi

# The diagnosis under a global config whose origin and value are paths: it
# names the keys and their scope, and neither path.
home="$work/diag-home"
mkdir -p "$home"
# One value spans two lines, a home on its second (`feature.note`, a key git
# does not read). Every line the diagnosis prints must have its own shape.
printf '[grep]\n\tthreads = 2\n[core]\n\tfsmonitor = %shook\n[feature]\n\tmanyFiles = false\n\tnote = "x\\n%sy"\n' "$mac" "$mac" >"$home/.gitconfig"
t="$work/shapes"
missing=("w3/trace-report.md:1: $linux_shape")
# Its fd 3 and its fd 2 both go to the file: nothing it says, on purpose or
# not, may hold a path.
(HOME="$home" && export HOME && unset XDG_CONFIG_HOME && diagnose_shapes) 3>"$work/diag" 2>&3 || true
# Each line the diagnosis prints, whole. The free text in it by design is a
# plant's name (relative, the test's own) and git's and bash's version
# strings; none may start a word with `/`.
gave_re='line [0-9]+ [a-z]+'
accounting_re='check-no-local-paths: git gave [0-9]+ line\(s\): [0-9]+ refused, [0-9]+ in [0-9]+ binary file\(s\), [0-9]+ marked, [0-9]+ placeholder'
shaped="^    \\| (ran: bash [0-9][^ /]*; (git version [0-9][^/]*|git: no version)|config: (system|global|local|worktree|command|<odd>) ([a-z0-9.-]+|<odd>) (true|false|-?[0-9]+|<set>)|expected: $accounting_re|actual:   ($accounting_re|none)|lost: [^ /][^ ]*: in the failing pass: (no trace written|no line of it|$gave_re(, $gave_re)*); after it: in the index [0-9]+ time\\(s\\) \\(ls-files exit [0-9]+\\), ([0-9]+ byte\\(s\\)|a symlink|no file) on disk, a later git grep -c: ([^ /][^ ]*:[0-9]+|no line) ?\\(exit [0-9]+\\)|[0-9]+ more not named were refused in the failing pass \\(its trace\\), before the scan failed|per run: a second run over the same tree \\(exit [0-9]+\\) named every one|per tree: a second run over the same tree \\(exit [0-9]+\\) missed [0-9]+ of them too|second run: [0-9]+ named; ($accounting_re|no accounting))$"
if ! grep -q -x -F '    | config: global grep.threads 2' "$work/diag" ||
	! grep -q -x -F '    | config: global core.fsmonitor <set>' "$work/diag" ||
	! grep -q -x -F '    | config: global feature.manyfiles false' "$work/diag" ||
	! grep -q -x -F '    | config: global feature.note <set>' "$work/diag" ||
	grep -q -F -e "$home" -e "$mac" "$work/diag" || grep -q -E '^    \| ran: .*/' "$work/diag" ||
	grep -q -v -E "$shaped" "$work/diag"; then
	# Its text may hold the planted paths: counts only.
	problem "diagnosis: it does not name the config keys by scope, or it prints a path ($(grep -c -F -e "$home" -e "$mac" "$work/diag" || true) line(s) with one; $(grep -c -v -E "$shaped" "$work/diag" || true) line(s) of no known shape)"
else
	ok diagnosis "keys by scope, no path"
fi

# The diagnosis over a tree that is gone: a `cd` that fails names its
# directory, and the diagnosis may not say it. One plant the trace refused,
# one it never saw (so its probes run, in the gone tree).
t="$work/gone-tree"
missing=("w3/trace-report.md:1: $linux_shape" "nowhere.md:1: $linux_shape")
(HOME="$home" && export HOME && unset XDG_CONFIG_HOME && diagnose_shapes) 3>"$work/diag" 2>&3 || true
if grep -q -F -e "$work" -e "$home" -e "$mac" "$work/diag" || grep -q -v -E "$shaped" "$work/diag" ||
	! grep -q -x -F '    | 1 more not named were refused in the failing pass (its trace), before the scan failed' "$work/diag" ||
	! grep -q -x -F '    | lost: nowhere.md: in the failing pass: no line of it; after it: in the index 0 time(s) (ls-files exit 128), no file on disk, a later git grep -c: no line (exit 1)' "$work/diag" ||
	! grep -q -x -F '    | per tree: a second run over the same tree (exit 1) missed 2 of them too' "$work/diag"; then
	problem "diagnosis-gone: over a tree that is gone it prints a path or a line of no known shape ($(grep -c -F -e "$work" -e "$home" -e "$mac" "$work/diag" || true) line(s) with a path; $(grep -c -v -E "$shaped" "$work/diag" || true) of no known shape)"
else
	ok diagnosis-gone "no path, every line its shape"
fi

if [ "$failed" -ne 0 ]; then
	printf 'test-check-no-local-paths: %d of %d case(s) failed\n' "$failed" "$((passed + failed))" >&2
	exit 1
fi
printf 'test-check-no-local-paths: ok (%d cases)\n' "$passed"
finished=1
