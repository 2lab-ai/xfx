#!/usr/bin/env bash
#
# Refuses to let a credential be committed.
#
# xfx's whole security story is that it reads a token from the environment,
# sends it to exactly one endpoint, and never writes it anywhere -- not into a
# session, not into a snapshot, not into a log. A token checked into the
# repository would make that story false before the product even runs, and the
# most common way one arrives is a debugging session that was never cleaned up.
#
# The scan is over *tracked* files: what a push would publish. It looks for the
# shapes real credentials have -- known issuer prefixes, PEM private keys, JWTs,
# and long bearer literals -- rather than for the names of environment
# variables, which this repository is supposed to talk about.

set -euo pipefail

# BSD grep -- macOS's -- puts `GREP_OPTIONS` in front of every argument list,
# so a `--exclude=*.md` there would skip every Markdown file this scan names,
# and the scan would report them clean. (GNU grep 3.8 ignores it.) The scan's
# options are its own.
unset GREP_OPTIONS

# Git takes where a repository is, which index it lists, where it writes
# objects and which config and attributes apply from its environment:
# `GIT_DIR`, `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`, `GIT_COMMON_DIR`,
# `GIT_CONFIG_*`, `GIT_ATTR_SOURCE` and more. One inherited from a caller --
# an exported shell, a hook -- made it list another index: with
# `GIT_INDEX_FILE` naming one without a file, the key in that file passed. With
# `GIT_OBJECT_DIRECTORY` or `GIT_COMMON_DIR`, the control below wrote into
# another repository. Inside a hook, git exports `GIT_DIR` and
# `GIT_INDEX_FILE` for the commit it is making. This scan reads every file's
# content from the working tree, never from an index, so it is a check of the
# checkout there too, and it lists the checkout's own index.
# This checks the checkout it is in, as CI does: that checkout's own
# `.git`, index and working tree. So every `GIT_*` goes before any git runs,
# read from the shell's own list of its variables, `${!GIT_@}`: no command to
# be missing (`compgen`, a readline builtin, is not in every bash). A name that
# is no identifier cannot be unset, and git reads none such.
# scripts/test-gate-env.sh holds it to that.
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

failures=0

fail() {
	printf 'check-no-secrets: %s\n' "$1" >&2
	failures=$((failures + 1))
}

# Where a listing is staged, so its producer's exit status is read rather than
# lost inside a process substitution.
listing="$(mktemp "${TMPDIR:-/tmp}/xfx-secret-listing.XXXXXX")"
trap 'rm -f "$listing"' EXIT

# Reads the NUL-separated names in "$listing" into the array named by $1.
read_listing() {
	local name
	eval "$1=()"
	while IFS= read -r -d '' name; do
		[ -n "$name" ] || continue
		eval "$1+=(\"\$name\")"
	done <"$listing"
}

# The files a push would publish, from `dir`: every tracked name into the array
# `files`, and those tracked but deleted from the working tree into `deleted`.
# Outside a checkout, the working tree minus build output, so the check still
# runs in an unpacked tarball.
#
# **NUL-delimited.** Without `-z`, `git ls-files` C-quotes a name with a tab, a
# newline, a quote or a byte past ASCII -- `"caf\303\251\tx.pyc"` -- and the
# quoted string is no file: `grep` cannot open it, and a rule that reads the
# name sees one that no longer ends in `.pyc`. Each name is read up to its NUL,
# as it is on disk.
#
# **Every tracked name**, whatever is in the working tree under it -- a symlink,
# a name deleted locally but still in the index: the name rules below are about
# what a push publishes. Which of them `grep` reads is [`scan`]'s to decide.
# A listing that did not finish is a failure, never a shorter list.
#
# Read into arrays with a loop rather than with `mapfile`, which is bash 4 and
# therefore absent from the bash macOS ships.
list_files() {
	local dir=$1 name
	files=()
	deleted=()
	# `core.fsmonitor`, from any config, runs a program during `ls-files`;
	# pinned off.
	if git -c core.fsmonitor=false -C "$dir" rev-parse --git-dir >/dev/null 2>&1; then
		if ! git -c core.fsmonitor=false -C "$dir" ls-files -z >"$listing"; then
			fail "git ls-files did not finish; the listing is incomplete"
		fi
		read_listing files
		if ! git -c core.fsmonitor=false -C "$dir" ls-files --deleted -z >"$listing"; then
			fail "git ls-files --deleted did not finish; the listing is incomplete"
		fi
		read_listing deleted
	else
		if ! (cd "$dir" && find . -type f -not -path './target/*' -not -path './.git/*' -print0) >"$listing"; then
			fail "find did not finish; the listing is incomplete"
		fi
		read_listing files
		local -a relative=()
		for name in ${files[@]+"${files[@]}"}; do
			relative+=("${name#./}")
		done
		files=(${relative[@]+"${relative[@]}"})
	fi
}

# Whether $1, missing from the working tree, is really gone: one of the names
# in `deleted`, under no directory that exists and cannot be searched. Git
# lists a name it could not look at, under a directory without search
# permission, as deleted too (after saying `cannot lstat` on its standard
# error), and that name is a file this scan cannot read rather than one that
# is not there.
is_deleted() {
	local name dir listed=no
	for name in ${deleted[@]+"${deleted[@]}"}; do
		if [ "$name" = "$1" ]; then
			listed=yes
			break
		fi
	done
	[ "$listed" = yes ] || return 1
	dir="$(dirname "./$1")"
	while [ "$dir" != "." ] && [ "$dir" != "/" ]; do
		if [ -d "$dir" ] && [ ! -x "$dir" ]; then
			return 1
		fi
		dir="$(dirname "$dir")"
	done
	return 0
}

list_files "$repo_root"

if [ "${#files[@]}" -eq 0 ]; then
	fail "no files to scan; the check would pass for the wrong reason"
	exit 1
fi

# `name:regex` pairs. Each pattern describes a credential's *shape*: matching one
# means a real secret, not a mention of one.
patterns=(
	"OpenAI-style key:sk-[A-Za-z0-9]{20,}"
	"GitHub token:gh[pousr]_[A-Za-z0-9]{20,}"
	"GitHub fine-grained token:github_pat_[A-Za-z0-9_]{20,}"
	"AWS access key id:AKIA[0-9A-Z]{16}"
	"Google API key:AIza[0-9A-Za-z_-]{30,}"
	"Slack token:xox[abprs]-[0-9A-Za-z-]{10,}"
	"private key block:-----BEGIN [A-Z ]*PRIVATE KEY-----"
	"JSON web token:eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\."
	"bearer literal:[Bb]earer [A-Za-z0-9_.=-]{24,}"
)

# --- the positive control ------------------------------------------------
#
# Every pattern is first run against a credential of its own shape, and must
# find it. Without this the whole script is one `grep` invocation away from
# being a green light that never looks at anything: a typo in a regex, a `grep`
# that does not take `-E` the way this one expects, an empty file list. A check
# whose failure mode is "silently passes" has to be able to prove it is awake.
#
# The samples are assembled at runtime rather than written out, because this
# script is itself a tracked file that the scan below reads. A literal sample
# would be a finding.
control="$(mktemp "${TMPDIR:-/tmp}/xfx-secret-control.XXXXXX")"
trap 'rm -f "$control" "$listing"' EXIT

filler="AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
digits="1234567890123456"
jwt_head="ey"
jwt_head="${jwt_head}J"
private_key="PRIVATE"
private_key="${private_key} KEY"
{
	printf 'sk-%s\n' "$filler"
	printf 'ghp_%s\n' "$filler"
	printf 'github_pat_%s\n' "$filler"
	printf 'AKIA%s\n' "$digits"
	printf 'AIza%s%s\n' "$filler" "$filler"
	printf 'xoxb-%s\n' "$filler"
	printf -- '-----BEGIN TEST %s-----\n' "$private_key"
	printf '%s%s.%s%s.%s\n' "$jwt_head" "$filler" "$jwt_head" "$filler" "$filler"
	printf 'Bearer %s\n' "$filler"
} >"$control"

for entry in "${patterns[@]}"; do
	name="${entry%%:*}"
	pattern="${entry#*:}"
	if ! grep -q -E -e "$pattern" "$control"; then
		fail "the '$name' pattern does not match its own sample; this scan is not working"
	fi
done

# --- the scan itself ---------------------------------------------------------
#
# One function, so the control below runs the same code over files of its own.

# Nor is Python's bytecode. A compiled `.pyc` records the absolute path of the
# source it was compiled from, so a tracked one publishes a local path -- the
# home directory, a worktree -- in a file nobody reads as text. `.gitignore`
# keeps the cache out of `git add`; this catches one added past it.
is_bytecode() {
	case "$1" in
	*.pyc | __pycache__/* | */__pycache__/*) return 0 ;;
	esac
	return 1
}

# The most bytes of file names one `grep` is given. Every name is an argument,
# and the kernel refuses an `exec` whose arguments and environment together
# pass `ARG_MAX` (E2BIG) -- which this repository's names alone came within
# 30 KB of, at about 12 000 files, under macOS's 1 MiB. Linux's limit is a
# quarter of the stack limit and never under 128 KiB, so 64 KiB of names
# leaves the other half of the smallest limit to the environment and to
# `grep`'s own arguments. It is assigned here, not read from the environment:
# a larger one is the E2BIG back.
batch_bytes=65536

# Prints how the operands "$@" divide into batches of at most `batch_bytes`,
# one `start:count` per batch, in order. A name costs its bytes, the NUL that
# ends it and the pointer to it. Bytes, not characters: `${#name}` counts
# characters in a UTF-8 locale, so it is taken in the C one, here only. A name
# longer than the whole budget is a batch of its own rather than none.
batch_bounds() {
	local LC_ALL=C
	local operand start=0 count=0 size=0 cost
	for operand in "$@"; do
		cost=$((${#operand} + 9))
		if [ "$count" -gt 0 ] && [ $((size + cost)) -gt "$batch_bytes" ]; then
			printf '%d:%d\n' "$start" "$count"
			start=$((start + count))
			count=0
			size=0
		fi
		count=$((count + 1))
		size=$((size + cost))
	done
	printf '%d:%d\n' "$start" "$count"
}

# Reports, through `fail`, what `grep` finds in the files "$@" name -- each one
# `./`-qualified by the caller, so a tracked file named `-` is that file and
# not standard input.
#
# One `grep` per batch of names (see `batch_bounds`), per pattern, and each
# pattern's batches judged together: a batch `grep` could not finish fails the
# pattern whichever batch it was, and every batch's hits are printed. `-H`,
# because a batch can be one name, and `grep` given one file prints its lines
# without the file's name.
grep_names() {
	local entry name pattern hits batch_hits status unreadable bounds bound start count covered=0
	# With no file named, `grep` reads its standard input -- which is not a
	# terminal here, so it would wait on it rather than find nothing.
	if [ "$#" -eq 0 ]; then
		fail "no files to scan; the check would pass for the wrong reason"
		return
	fi
	# The batches must be the operands, each once and in order: a gap or a
	# batch lost here would be files this scan reports clean without reading.
	bounds="$(batch_bounds "$@")"
	for bound in $bounds; do
		start="${bound%%:*}"
		count="${bound#*:}"
		if [ "$start" -ne "$covered" ] || [ "$count" -lt 1 ]; then
			break
		fi
		covered=$((covered + count))
	done
	if [ "$covered" -ne "$#" ]; then
		fail "the batches cover $covered of $# files; the check would pass for the wrong reason"
		return
	fi
	for entry in "${patterns[@]}"; do
		name="${entry%%:*}"
		pattern="${entry#*:}"
		hits=""
		unreadable=no
		for bound in $bounds; do
			start="${bound%%:*}"
			count="${bound#*:}"
			# Status 1 is "no match"; 2 is a file `grep` could not read, which
			# is a file this scan did not look at. Past 2 is a `grep` that did
			# not run or did not finish -- 126 is the shell's E2BIG -- and the
			# same: a batch not looked at.
			status=0
			batch_hits="$(grep -H -n -E -e "$pattern" -- "${@:$((start + 1)):$count}" 2>&1 </dev/null)" || status=$?
			if [ "$status" -ge 2 ]; then
				unreadable=yes
			fi
			if [ -n "$batch_hits" ]; then
				hits="${hits:+$hits
}$batch_hits"
			fi
		done
		if [ "$unreadable" = yes ]; then
			fail "grep could not read every file for the '$name' pattern:"
			printf '%s\n' "$hits" >&2
		elif [ -n "$hits" ]; then
			fail "possible $name:"
			printf '%s\n' "$hits" >&2
		fi
	done
}

# Reports, through `fail`, every problem among the tracked names in "$@",
# relative to the current directory: the name rules over every one of them,
# and `grep` over the ones that are regular files here. A symlink's content is
# a path and is not read; a name deleted locally is not either. A name that is
# none of these -- it cannot even be looked at, under a directory that cannot
# be searched -- is a failure: it is a file this scan did not read.
scan() {
	local file
	local -a operands=()
	for file in "$@"; do
		# A `.env` file is never a source file here: the design keeps
		# credentials in the process environment, so one that is tracked is a
		# mistake whatever it contains.
		# The last component by expansion, not `basename`: a name under a
		# directory called `-x` would be its option.
		case "${file##*/}" in
		.env | .env.*)
			fail "$file is tracked; credential files belong outside the repository"
			;;
		esac
		if is_bytecode "$file"; then
			fail "$file is tracked; Python's bytecode records local paths and belongs outside the repository"
		fi
		if [ -L "./$file" ]; then
			continue
		elif [ -f "./$file" ]; then
			operands+=("./$file")
		elif [ -e "./$file" ] || is_deleted "$file"; then
			continue
		else
			fail "$file is tracked and cannot be read"
		fi
	done
	grep_names ${operands[@]+"${operands[@]}"}
}

# The bytecode rule's positive control, as the patterns have theirs: it must
# name the file a run of `scripts/fxcap/redact.py` leaves behind, and must not
# name the source beside it.
for sample in scripts/fxcap/__pycache__/redact.cpython-314.pyc scripts/fxcap/ledger.pyc __pycache__/x; do
	if ! is_bytecode "$sample"; then
		fail "the bytecode rule does not match '$sample'; this scan is not working"
	fi
done
if is_bytecode scripts/fxcap/redact.py; then
	fail "the bytecode rule matches a source file; this scan is not working"
fi

# And the whole path's, from the listing to the refusal: a checkout of its own,
# with `GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` unset so a run from a
# hook cannot stage into the repository that ran it, and standard input closed.
# It tracks, and `list_files` and `scan` must refuse exactly as they would the
# repository's files:
#   - a key-shaped string in a file whose name has a tab and a byte past ASCII
#     in it, and a `.pyc` named the same way outside `__pycache__` -- the names
#     `git ls-files` would quote;
#   - a key-shaped string in a file named `-`, which `grep` would otherwise
#     read as its standard input;
#   - a `.pyc` deleted from the working tree but still in the index;
#   - a `.env` that is a symlink to nothing, which the name rule refuses
#     whatever is under the name, and a `.env` under a directory named `-x`.
# And `grep`'s failure to read a file (status 2) must be a failure.
#
# All of it one name to a batch, so it crosses every batch boundary: the keys
# above are in the first batch (`-`) and the last (the quoted name), and the
# file `grep` cannot read is a batch before one it can, whose key must be
# printed too, under the read failure.
# `scripts/test-check-no-secrets.sh` does the same over a tree that does not
# fit on one command line.
sample_dir="$(mktemp -d "${TMPDIR:-/tmp}/xfx-secret-names.XXXXXX")"
trap 'rm -f "$control" "$listing"; rm -rf "$sample_dir"' EXIT
odd_text="$(printf 'caf\303\251\tkey.txt')"
odd_bytecode="$(printf 'caf\303\251\tcache.pyc')"
printf 'sk-%s\n' "$filler" >"$sample_dir/$odd_text"
printf 'x' >"$sample_dir/$odd_bytecode"
printf 'sk-%s\n' "$filler" >"$sample_dir/-"
printf 'x' >"$sample_dir/gone.pyc"
ln -s nowhere "$sample_dir/.env"
mkdir -p -- "$sample_dir/-x"
printf 'A=1\n' >"$sample_dir/-x/.env"
sample_tracked=no
sample_report="$(
	unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE
	# A throwaway repository: no global or system config of the caller -- a
	# hook, a template, an fsmonitor -- runs in it. (No apostrophe in a comment
	# here: bash 3.2 reads one inside a command substitution as a quote.)
	export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
	if command -v git >/dev/null 2>&1 && git -c core.fsmonitor=false -C "$sample_dir" init -q 2>/dev/null &&
		git -c core.fsmonitor=false -C "$sample_dir" add -f -- "$odd_text" "$odd_bytecode" ./- gone.pyc .env ./-x/.env; then
		rm -f "$sample_dir/gone.pyc"
		printf 'tracked\n'
	fi
	failures=0
	batch_bytes=1
	list_files "$sample_dir"
	cd "$sample_dir" && scan ${files[@]+"${files[@]}"} 2>&1
	grep_names ./does-not-exist ./- 2>&1
)" </dev/null || true
case "$sample_report" in
tracked*) sample_tracked=yes ;;
esac
case "$sample_report" in
*"possible OpenAI-style key:"*"./$odd_text:1:"*) ;;
*) fail "a key in a file whose name git would quote was not found; this scan is not working" ;;
esac
case "$sample_report" in
*"$odd_bytecode is tracked; Python's bytecode"*) ;;
*) fail "a .pyc whose name git would quote was not refused; this scan is not working" ;;
esac
case "$sample_report" in
*"possible OpenAI-style key:"*"./-:1:"*) ;;
*) fail "a key in a file named '-' was not found; this scan is not working" ;;
esac
if [ "$sample_tracked" = yes ]; then
	case "$sample_report" in
	*"gone.pyc is tracked; Python's bytecode"*) ;;
	*) fail "a tracked .pyc deleted from the working tree was not refused; this scan is not working" ;;
	esac
	case "$sample_report" in
	*"check-no-secrets: .env is tracked; credential files"*) ;;
	*) fail "a tracked .env that is a symlink to nothing was not refused; this scan is not working" ;;
	esac
fi
case "$sample_report" in
*"check-no-secrets: -x/.env is tracked; credential files"*) ;;
*) fail "a .env under a directory named '-x' was not refused; this scan is not working" ;;
esac
case "$sample_report" in
*"grep could not read every file"*"does-not-exist"*) ;;
*) fail "a file grep could not read was not a failure; this scan is not working" ;;
esac
case "$sample_report" in
*"grep could not read every file for the 'OpenAI-style key' pattern:"*"does-not-exist"*"./-:1:"*) ;;
*) fail "a key in the batch after one grep could not read was not printed; this scan is not working" ;;
esac

scan "${files[@]}" </dev/null

if [ "$failures" -ne 0 ]; then
	printf 'check-no-secrets: %d problem(s) found\n' "$failures" >&2
	exit 1
fi

printf 'check-no-secrets: ok (%d tracked file(s) scanned)\n' "${#files[@]}"
