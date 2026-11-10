#!/usr/bin/env bash
#
# Refuses to let a path that belongs to one machine be committed.
#
# This repository is public. A capture taken on a worker's machine, a receipt
# script, a README written beside a run, carries that machine's scratch root,
# its home directory and with it its user's name, and once pushed they are
# published. They came in faster than a review could catch them: on
# 2026-11-09 `main` held 156 files with one machine's scratch root in them,
# every image capture embedded the harness's run directory, and four units in
# one night committed scratch paths in receipt scripts and READMEs.
# scripts/fxcap/capture.sh now writes a run's own roots as placeholders, each
# as wide as its root (`<work______>`, `<scratch___>`, `<tmp___>`, `<h__>`:
# scripts/fxcap/roots.py); this check is what holds the tree to it.
#
# It looks for the shapes such a path has, never for one machine's names:
#
#   /Users/<name>          a macOS home directory
#   /home/<name>           a Linux home directory
#   /tmp/claude-<digits>   a harness's per-user scratch root, also as
#                          /private/tmp/claude-<digits> on macOS, and a
#                          `claude-<digits>/` at a row's start, after a
#                          control sequence or after a character outside
#                          `A-Za-z0-9._~+-` and `/` (a space, a backtick, a
#                          paren), where a binary or a README broke such a
#                          root after its `tmp/` -- not inside an ASCII name
#                          or after a `/` (`models/claude-3/`)
#   /var/folders/          macOS's per-user temporary root, also as  # local-path-ok
#                          /private/var/folders/                      # local-path-ok
#
# A home directory counts where an absolute path starts -- at the start of a
# line, after a character outside `A-Za-z0-9._~+-`, or after a terminal
# control sequence (a capture's .ansi colours a path with an SGR sequence,
# which ends in `m`) -- and ends with its name: before a `/`, any character
# outside `A-Za-z0-9._-`, or the end of the line. So `a/home/b/` (a relative
# path) is not one, a user name starts with a letter, a digit or `_`
# (`/home/.fx/` is not one), and a redirect into a home directory, or a bare
# `cd` into one, is one.
#
# Those sets are a heuristic, not what a path name may hold: `@`, `=` and a
# letter past ASCII are legal in a name, so `cache@claude-<n>/x` and an
# accented letter before `claude-<n>/` match the broken-root shape though they
# are relative names. The check fails toward refusal there, on purpose, and
# its test pins both.
#
# It is not every path of a machine. A Windows path, `~user`, a path a URL
# percent-encodes, a home directory broken across two rows and a scratch root
# broken anywhere but after its `tmp/` are not found by their shape.
#
# Two exceptions, and no others -- no directory is exempt:
#
# - **A placeholder**, taken out of a line before the shapes are matched
#   again, so a line that holds one and a real path as well is still refused:
#   a home directory that is nobody's (`allowed` below: a test's `/home/u`, a
#   README's `/home/you`), and a path that continues one of capture.sh's
#   placeholders, at any width or in its short form (`<scratch___>/home/...`,
#   a run's HOME).
# - **A line that ends in a `local-path-ok` comment**, for a fixture that
#   must hold such a shape on purpose: a redaction test's input, the shapes
#   this file's own documentation names. The mark follows a comment opener
#   (`#`, `//`, `--`, `;`, `<!--`, `/*`) on that line and ends it, a reason
#   after a `:` allowed; it excuses that line only. In a capture no mark
#   excuses anything: the text is a binary's, not a fixture's. A capture is a
#   file of a kind capture.sh writes (a `.txt`, `.ansi`, `.wire`, `.json`,
#   `.log` and the rest) or one under a `*-bodies/` directory, whatever its
#   name; or a file by the name capture.sh gives every file it writes,
#   `<bin>-<scenario>-<rows>x<cols>-...`, or under a directory so named,
#   wherever it is -- a `/trace` report's `.md` among them. That name is
#   reserved for captures (capture.sh says so too): a capture renamed out of
#   it is read by its kind alone (a report renamed `notes.md` is authored
#   text, whose marks excuse their lines; a shape on an unmarked line is
#   still refused), and an authored file named into it is a capture.
#
# The scan is over *tracked* files -- what a push publishes -- read from the
# working tree, every one as text: a binary file can carry a path as well as
# a text one, and git's attributes, which decide "binary", come from config a
# caller holds. A hit in a file with a NUL anywhere in it cannot be marked, so
# it is refused by name: a shell variable cannot hold a NUL (bash 3.2 cuts
# the line there), so this shell cannot read such a line as git did.
#
# It is one `git grep` over the tree, which gives every line that holds a
# shape with its file and number, and that one answer is all the scan reads.
# No file name goes on a command line, so there is no `ARG_MAX` to batch
# under, as scripts/check-no-secrets.sh must; the repository-level guards are
# scripts/check-xfx-identity.sh's, which also scans with `git grep` (that
# file says why each is there).
#
# Every line git gives is accounted for, or the check fails: a shape is
# refused, a mark excuses it, or a placeholder taken out of it was all it
# held. A line git matched in which this shell then finds no shape and takes
# no placeholder out is refused too, never counted as passing: git and this
# shell read it differently (they match with two regex engines), and an
# answer that depends on which of them is right is no answer. The check used
# to grep the tree for the files and then each file for its lines, and a file
# the first said held a shape and the second gave no line for was dropped
# without a word, as was a line the shell read with no shape in it. On
# 2026-11-10 its test's tree lost two planted files on one CI run (macOS 15,
# git 2.56.0) somewhere on that path, and named them on every other run.
#
# What it prints for a hit is the file, the line and the shape, never the
# line's text: CI's log is public too.

set -euo pipefail

# One reading of every pattern and every byte, on every row: the matching
# below is bash's own (`[[ =~ ]]`, the system's ERE), and a UTF-8 locale gives
# a range like `A-Z` more members on some systems than on others.
export LC_ALL=C

# Git takes where a repository is, which index it lists and which config
# applies from `GIT_DIR`, `GIT_INDEX_FILE`, `GIT_COMMON_DIR`, `GIT_CONFIG_*`
# and more; one inherited from a caller would make this scan another tree.
# Every `GIT_*` goes before any git runs, as in check-xfx-identity.sh.
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done
unset GREP_OPTIONS

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

failures=0

fail() {
	printf 'check-no-local-paths: %s\n' "$1" >&2
	failures=$((failures + 1))
}

# `name:regex` pairs, POSIX ERE. Each is written so that its own text matches
# none of them -- `/var/folder[s]/` matches only the bracket-free spelling --
# because this file is tracked, and is scanned with the rest.
esc=$'\033'
control="${esc}\\[[0-9;:?]*[@-~]"
start="(^|[^A-Za-z0-9._~+-]|${control})"
user='[A-Za-z0-9_][A-Za-z0-9._-]*'
end='([^A-Za-z0-9._-]|$)'
patterns=(
	"a macOS home directory:${start}/User[s]/${user}${end}"
	"a Linux home directory:${start}/hom[e]/${user}${end}"
	"a harness scratch root:/tmp/claud[e]-[0-9]+"
	"a harness scratch root, broken after its tmp/:(^|[^A-Za-z0-9._~+/-]|${control})claud[e]-[0-9]+/"
	"a macOS per-user temporary root:/var/folder[s]/"
)

# The documented placeholders: home directories of nobody, each `/<root>/<name>`
# taken out where its name ends (as a shape's does).
allowed=(
	/home/u        # src/permission/policy.rs: the protected dotfiles of a user `u`
	/Users/u       #   and the same on macOS
	/Users/me      # src/tui/hyperlink.rs, src/tui/image_paths.rs: a screenshot's path
	/home/me       # src/mcp/transport.rs: a child's HOME the test sets
	/Users/example # src/config.rs: a path the test keeps as it is
	/home/someone  # src/llmux/setup.rs: a config path the test reads back
	/Users/someone # scripts/fxcap/test_sweep.py: the redactor's own input
	/home/you      # README.md: the example workspace
	/home/runner   # GitHub-hosted runners' home, and the user train-6's Linux container makes
)
allowed_re="("
for entry in "${allowed[@]}"; do
	allowed_re+="${entry}|"
done
allowed_re="${allowed_re%|})${end}"
# capture.sh's placeholders (scripts/fxcap/roots.py PLACEHOLDER: a name, or
# its letter, filled with `_` to the root's width), as given and resolved:
# what follows one is inside the run, a run's HOME among it.
placeholder_home_re="<((work|scratch|scenarios|bun|tmp|home)(:real)?_*|[wscbthWSCBTH]_+)>/(home|Users)"
marker=local-path-ok
# The mark, as the comment that ends a line.
marked_re="(^|[[:space:]])(#|//|--|;|<!--|/[*]).*${marker}(:.*)?([[:space:]]*(-->|[*]/))?[[:space:]]*$"

# Whether the text $1 matches the ERE $2, as `[[ =~ ]]` says, BASH_REMATCH and
# all. A pattern this shell cannot compile is status 2, which an `if` reads as
# "no match"; that is a scan that did not run, so it is kept in
# `uncompiled` for the scan to fail on, and the answer is still "no".
uncompiled=""
rematch() {
	local status=0
	[[ $1 =~ $2 ]] || status=$?
	if [ "$status" -gt 1 ]; then
		uncompiled="$2"
	fi
	return "$status"
}

# Whether a file is a capture, whose text no mark excuses: by its name, any
# file scripts/fxcap/capture.sh writes or a directory it fills
# (`<bin>-<scenario>-<rows>x<cols>-<label>...`, a `/trace` report's `.md`
# and a request body's directory among them: a name reserved for captures,
# as the header says); and by its kind, whatever its name.
capture_name_re='(^|/)x?fx-[^/]*-[0-9]{1,4}x[0-9]{1,4}-'
is_capture() {
	if rematch "$1" "$capture_name_re"; then
		return 0
	fi
	case "$1" in
	*-bodies/* | *.txt | *.ansi | *.wire | *.json | *.log | *.files | *.clip | *.clipfile | *.title | *.cursor | *.bells | *.sockets | *.port) return 0 ;;
	esac
	return 1
}

# Git for the scans. Config the `GIT_*` drop leaves can still steer it:
# `core.fsmonitor` runs a program; a checkout-local `core.worktree` reads
# another tree (`--work-tree=.` names this one); `grep.column`,
# `grep.fullName` and `color.grep` change what a line of output looks like;
# `submodule.recurse` reads other repositories.
scan_git() {
	local root="$1"
	shift
	git -c core.fsmonitor=false -c grep.column=false -c grep.fullName=false \
		-c color.grep=never -c submodule.recurse=false -C "$root" --work-tree=. "$@"
}

# Git for the throwaway control repository: no global or system config.
scratch_git() {
	GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false "$@"
}

grep_args=()
for entry in "${patterns[@]}"; do
	grep_args+=(-e "${entry#*:}")
done

# Counts of the last scan's lines: every line git gave, those its exceptions
# excused (for the line that says it passed), and those it refused.
given=0
marked=0
placeheld=0
refused=0
binary_files=0

# Every hit in the tracked files of `root`, one `<path>:<line>: <shape>` per
# line on standard output. Returns non-zero, having said why on standard
# error, if the scan could not finish -- never an empty answer for a scan that
# did not run. Failure from `git grep` is a status above 1 or anything on its
# standard error (bad config exits 128; an unreadable tracked file is only
# `error: failed to stat`, with status 1).
scan_tree() {
	local root="$1" path number text bare entry hit current="" binary=no markable=no probe removed status
	local given_lines="$work/given" errors="$work/errors"
	given=0
	marked=0
	placeheld=0
	refused=0
	binary_files=0
	status=0
	# `-z`: `<path>NUL<number>NUL<text>LF` for every line, the path as it is.
	scan_git "$root" grep -a --no-color -n -z -E "${grep_args[@]}" -- >"$given_lines" 2>"$errors" || status=$?
	if [ "$status" -gt 1 ] || [ -s "$errors" ]; then
		printf 'check-no-local-paths: git grep could not search %s (status %d):\n' "$root" "$status" >&2
		cat "$errors" >&2
		return 1
	fi
	while IFS= read -r -d '' path; do
		if ! IFS= read -r -d '' number || ! IFS= read -r text; then
			printf 'check-no-local-paths: git grep gave a line of %s that ends early\n' "$path" >&2
			return 1
		fi
		case "$number" in
		'' | *[!0-9]*)
			printf 'check-no-local-paths: git grep gave a line of %s with no line number\n' "$path" >&2
			return 1
			;;
		esac
		given=$((given + 1))
		# A file's lines come together, in the index's order.
		if [ "$path" != "$current" ]; then
			current="$path"
			# Read from the file named, never from what this loop reads.
			if ! probe="$(tr -d -c '\000' <"$root/$path" | wc -c)"; then
				printf 'check-no-local-paths: could not read %s\n' "$root/$path" >&2
				return 1
			fi
			binary=no
			if [ "$probe" -ne 0 ]; then
				binary=yes
				binary_files=$((binary_files + 1))
				printf '%s: (a binary file, which cannot be marked): a local path\n' "$path"
			fi
			markable=yes
			if is_capture "$path"; then
				markable=no
			fi
		fi
		if [ "$binary" = yes ]; then
			continue
		fi
		if [ "$markable" = yes ] && rematch "$text" "$marked_re"; then
			marked=$((marked + 1))
			continue
		fi
		# A space after the line: every match below then ends in the
		# character after it, never at the end of the text, so the first
		# place its text occurs is the place it matched. (Without it, a
		# match at the line's end, a bare documented home, was cut at its
		# first occurrence -- inside an earlier, longer home that starts
		# with the same letters.)
		bare="$text "
		# Each placeholder home, then each documented one where its name
		# ends (keeping the character after it), one at a time, the
		# leftmost first. Rebuilt from either side of the match: bash 3.2
		# keeps the quotes of a quoted replacement, and bash 5.2 reads `&`
		# in a bare one.
		removed=0
		while rematch "$bare" "$placeholder_home_re"; do
			found="${BASH_REMATCH[0]}"
			bare="${bare%%"$found"*}${bare#*"$found"}"
			removed=$((removed + 1))
		done
		while rematch "$bare" "$allowed_re"; do
			found="${BASH_REMATCH[0]}" after="${BASH_REMATCH[2]}"
			bare="${bare%%"$found"*}${after}${bare#*"$found"}"
			removed=$((removed + 1))
		done
		hit=""
		for entry in "${patterns[@]}"; do
			if rematch "$bare" "${entry#*:}"; then
				hit="${hit:+$hit, }${entry%%:*}"
			fi
		done
		if [ -n "$uncompiled" ]; then
			printf 'check-no-local-paths: this shell could not compile a pattern of its own (status 2 from [[ =~ ]]), so it cannot say what %s holds\n' "$path" >&2
			return 1
		fi
		if [ -z "$hit" ] && [ "$removed" -eq 0 ]; then
			# Git matched a shape here and this shell sees none, nor a
			# placeholder that held it: they read the line differently.
			hit='a line git matched in which this shell finds no shape and no placeholder'
		fi
		if [ -n "$hit" ]; then
			refused=$((refused + 1))
			printf '%s:%s: %s\n' "$path" "$number" "$hit"
		else
			placeheld=$((placeheld + 1))
		fi
	done <"$given_lines"
	if [ -n "$uncompiled" ]; then
		printf 'check-no-local-paths: this shell could not compile a pattern of its own (status 2 from [[ =~ ]])\n' >&2
		return 1
	fi
}

work="$(mktemp -d "${TMPDIR:-/tmp}/xfx-local-paths.XXXXXX")"
trap 'rm -rf "$work"' EXIT

# --- the positive control ----------------------------------------------------
#
# A check whose failure mode is "silently passes" proves it is awake before
# its green is worth anything: the same scan, over a throwaway repository
# that holds every shape, and every form that must pass, and reports exactly
# the former. Its paths are assembled here, for the reason the patterns are
# bracketed: this file is scanned too.

control="$work/control"
mkdir -p "$control"
scratch_git -C "$control" init -q
s='/'
mac="${s}Users${s}alice${s}"
linux="${s}home${s}bob${s}"
harness="${s}private${s}tmp${s}claude-501"
folders="${s}private${s}var${s}folders${s}ab${s}T${s}"
{
	printf 'cd %srepo\n' "$mac"           # 1
	printf 'HOME=%s\n' "$linux"           # 2
	printf 'TMPDIR=%s/x\n' "$harness"     # 3
	printf 'file://%sshot.png\n' "$folders" # 4
	printf '%s\n' "${s}tmp${s}claude-1000${s}x" # 5
	printf 'a%sb and %s\n' "$mac" "${s}home${s}u${s}" # 6: a relative path, then a placeholder
	printf '%s and %s\n' "${s}home${s}u${s}" "$linux" # 7: a placeholder does not excuse a real path
	printf '%s # %s\n' "$folders" "$marker" # 8: marked, the comment that ends the line
	printf '%s\n' "$linux"                # 9: the mark above is its own line's only
	printf '<scratch>%shome%s.fx%smcp.json fxcap.Ab12Cd%shome%s.xfx%s\n' "$s" "$s" "$s" "$s" "$s" "$s" # 10
	printf '%s<name>%s %s<digits> <work>%sshot.png\n' "${s}Users${s}" "$s" "${s}private${s}tmp${s}claude-" "$s" # 11
	printf '\033[38;5;245m%sx\033[39m\n' "$linux" # 12: a coloured path, after its SGR sequence's `m`
	printf 'log >%sx\n' "$linux"             # 13: a redirect is no placeholder
	printf 'cd %s\n' "${s}Users${s}alice"    # 14: a home with no slash after it
	printf '<scratch____>%shome%su%sx <work:real_>%sUsers%scarol%s <h__>%shome%sz\n' "$s" "$s" "$s" "$s" "$s" "$s" "$s" "$s" # 15: inside a run
	printf '  \033[38;5;250mclaude-501%sfxcap.Ab12Cd%swork\n' "$s" "$s" # 16: a root broken after its tmp/
	printf 'see %sx %s and more\n' "$mac" "$marker" # 17: a mark that does not end the line
	printf 'cp x %subuntu%sy; cd %su\n' "${s}home${s}" "$s" "${s}home${s}" # 18: a placeholder home ends the line
} >"$control/lines.md"
printf 'trace %sx # %s\n' "$linux" "$marker" >"$control/fx-trace-40x120-report-1.md" # a report: a capture
printf 'x\000%s\n' "$mac" >"$control/blob.bin"
mkdir -p "$control/x-bodies"
printf '%s # %s\n' "$linux" "$marker" >"$control/x-bodies/001.json" # a mark in a capture
scratch_git -C "$control" add -A -f
want="blob.bin: (a binary file, which cannot be marked): a local path
lines.md:1: a macOS home directory
lines.md:2: a Linux home directory
lines.md:3: a harness scratch root
lines.md:4: a macOS per-user temporary root
lines.md:5: a harness scratch root
lines.md:12: a Linux home directory
lines.md:13: a Linux home directory
lines.md:14: a macOS home directory
lines.md:16: a harness scratch root, broken after its tmp/
lines.md:17: a macOS home directory
lines.md:18: a Linux home directory
fx-trace-40x120-report-1.md:1: a Linux home directory
lines.md:7: a Linux home directory
lines.md:9: a Linux home directory
x-bodies/001.json:1: a Linux home directory"
# Into a file, not through `$( )`: the counts are this shell's.
if ! scan_tree "$control" >"$work/got"; then
	fail 'the scan could not run in its own control; this scan is not working'
elif [ "$(sort "$work/got")" != "$(printf '%s\n' "$want" | sort)" ]; then
	fail 'the scan does not report exactly the planted paths in its own control; this scan is not working:'
	cat "$work/got" >&2
elif [ "$marked" -ne 1 ] || [ "$placeheld" -ne 2 ]; then
	fail "the control's marked line and its two placeholder lines were not counted ($marked marked, $placeheld placeholder); this scan is not working"
fi

# --- the scan itself ---------------------------------------------------------
#
# The repository git finds from here must be the one this directory's own
# `.git` names, compared as files (check-xfx-identity.sh says why a ceiling
# or a path comparison does not hold): a copy of xfx inside another
# repository would otherwise be scanned as the parent's paths, none of which
# is here, and pass.
if ! discovered="$(scan_git . rev-parse --absolute-git-dir 2>"$work/errors")"; then
	fail 'not a git checkout, or git cannot open it; this check can only speak about the tracked files of the checkout it is in:'
	cat "$work/errors" >&2
	exit 1
fi
if ! own="$(git -c core.fsmonitor=false --git-dir="$repo_root/.git" rev-parse --absolute-git-dir 2>"$work/errors")"; then
	fail "not a git checkout of its own: the root's .git cannot be opened:"
	cat "$work/errors" >&2
	exit 1
fi
if ! [ "$discovered" -ef "$own" ]; then
	fail "not a git checkout of its own: git finds $discovered, the root's .git names $own"
	exit 1
fi

# An entry with either index bit is one `git grep` does not read from the
# working tree (skip-worktree: not at all), and a tracked file gone from the
# working tree, or replaced by a directory or a symlink, is one it skips
# without a word. Both are refused, by name, as check-xfx-identity.sh does.
if ! scan_git . ls-files -v -z >"$work/tracked" 2>"$work/errors" || [ -s "$work/errors" ]; then
	fail 'git ls-files could not list this checkout:'
	cat "$work/errors" >&2
	exit 1
fi
tracked_count=0
flagged=()
while IFS= read -r -d '' entry; do
	tracked_count=$((tracked_count + 1))
	case "$entry" in
	S\ * | [[:lower:]]\ *) flagged+=("${entry#??}") ;;
	esac
done <"$work/tracked"
if [ "${#flagged[@]}" -ne 0 ]; then
	fail 'tracked files marked skip-worktree or assume-unchanged, which git grep does not read from the working tree; clear the bit (git update-index --no-skip-worktree --no-assume-unchanged -- <file>, or git sparse-checkout disable):'
	printf '%s\n' "${flagged[@]}" >&2
	exit 1
fi
if [ "$tracked_count" -eq 0 ]; then
	fail 'no tracked files to scan; the check would pass for the wrong reason'
	exit 1
fi

# The scan. A tracked file missing from the working tree, or there as a
# directory or a symlink, is one `git grep` passes over without a word (a file
# it cannot stat for ENOENT it does not report), whether it was gone before
# the scan or went while it ran; so the look for one comes after the scan,
# and refuses it by name.
scan_status=0
scan_tree . >"$work/found" || scan_status=$?
if ! scan_git . diff-files --name-only -z --diff-filter=DT >"$work/deleted" 2>"$work/errors" || [ -s "$work/errors" ]; then
	fail 'git diff-files could not list the tracked files missing from this checkout:'
	cat "$work/errors" >&2
	exit 1
fi
if [ -s "$work/deleted" ]; then
	fail 'tracked files missing from the working tree, or there as a directory or a symlink, which git grep would skip; restore or remove them (git restore -- <file>, or git rm -- <file>):'
	tr '\000' '\n' <"$work/deleted" >&2
	exit 1
fi
if [ "$scan_status" -ne 0 ]; then
	fail 'the scan could not finish, so it found nothing it can vouch for'
elif [ -s "$work/found" ]; then
	fail "a path that belongs to one machine is in tracked content (write it as a placeholder such as <tmp>, <home> or <work>, derive it from \$TMPDIR or \$HOME at run time, or mark a fixture's line $marker):"
	cat "$work/found" >&2
	# What became of every line git gave: a log of a failure tells a line
	# git never gave from one this check passed.
	printf 'check-no-local-paths: git gave %d line(s): %d refused, %d in %d binary file(s), %d marked, %d placeholder\n' \
		"$given" "$refused" "$((given - refused - marked - placeheld))" "$binary_files" "$marked" "$placeheld" >&2
fi

if [ "$failures" -ne 0 ]; then
	printf 'check-no-local-paths: %d problem(s) found\n' "$failures" >&2
	exit 1
fi

printf 'check-no-local-paths: ok (%d tracked file(s) scanned; %d marked fixture line(s), %d placeholder line(s))\n' \
	"$tracked_count" "$marked" "$placeheld"
