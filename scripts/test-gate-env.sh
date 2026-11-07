#!/usr/bin/env bash
#
# Proves that no gate takes its grep options, or its repository, from the
# environment.
#
#   scripts/test-gate-env.sh
#
# Two kinds of case: grep's `GREP_OPTIONS`, below, and git's own variables and
# attribute files, in the second half.
#
# BSD grep -- macOS's, on the macOS CI rows and on every Mac this repository
# is worked on -- puts `GREP_OPTIONS` in front of every argument list it is
# given. A gate that decides by grep then decides by whatever that variable
# says. With `-q`, a grep for `todo!` prints nothing. With `-v`, "the README
# carries this line" is true of any README with a second line, and "the test
# result line says 1 passed" is true of any output with a second line. GNU
# grep 3.8 ignores the variable. Each gate below unsets it, and this holds
# them to that.
#
# Each case builds a scratch tree with one planted violation, runs the gate
# with the `GREP_OPTIONS` value that let that violation through before the
# gate unset the variable, and requires the gate to refuse it with its own
# message. A gate that ignores the variable -- because it unsets it, or
# because this grep ignores it -- is then a plain run, and must report the
# violation; so the same case is the RED where grep reads the variable and
# proves only the plant where it does not. This test prints which.
#
# Where the gate runs cargo, a `cargo` placed first on PATH prints what a
# renamed test would produce, so nothing is built.
#
# check-no-secrets.sh has its own case, `grep-options`, in
# scripts/test-check-no-secrets.sh.
#
# Git takes where a repository is, which index it lists, where it writes
# objects and which config and attributes apply from `GIT_DIR`,
# `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`, `GIT_COMMON_DIR`, `GIT_CONFIG_*`,
# `GIT_ATTR_SOURCE` and the rest. Before the gates dropped them, a caller's
# made check-xfx-identity.sh and check-no-secrets.sh pass a planted violation,
# and made all three git-reading gates write into a decoy repository. And
# attributes from a config, `.git/info/attributes` or an untracked
# `.gitattributes` hid a Cargo.lock from the identity scan. Each git case
# runs a gate on a fixture with one violation, with one variable or one
# attribute file set, and requires the violation refused and the decoy's
# `.git` unchanged: the same entries, of the same type, every regular file's
# checksum and execute bit and every symlink's target. That shows no lasting
# write; a write undone before the case ends is not seen.
#
# Round 2 adds: the entry points outside the three gates (the workflow steps
# that run git, build.rs, test-check-no-secrets.sh), the identity scan
# failing closed when git cannot finish it, and config the GIT_* drop leaves
# -- a checkout-local `core.worktree`, and a HOME config whose
# `core.fsmonitor` and `core.hooksPath` run programs. A program run from
# config, or a `GIT_TRACE` file written, fails any case.
#
# Round 2b adds: a copy with no repository of its own inside one that tracks
# it (a vendored subtree), which `--work-tree=.` had turned into a silent
# pass; a linked worktree, where the identity scan and build.rs must still
# work; and the remedy the index-bit refusal names. And the nested run of
# test-check-no-secrets.sh below is CI's only full run of it.
#
# Requires git, ruby (check-preview-contract.sh) and rustc (build.rs), and a
# user that cannot read a mode-000 file (not root).

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# This file's own greps read the gates' output with options of their own,
# and its own gits build scratch repositories and nothing else; the cases
# hand a gate its variables explicitly.
unset GREP_OPTIONS
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done
# Git for this test's own scratch repositories: no global or system config,
# so no hook, template or fsmonitor of the caller's runs in them.
sg() {
	GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false "$@"
}

work="$(mktemp -d "${TMPDIR:-/tmp}/xfx-gate-env.XXXXXX")"
trap 'rm -rf "$work"' EXIT HUP INT TERM

failures=0
passed=0

problem() {
	printf 'test-gate-env: %s\n' "$1" >&2
	failures=$((failures + 1))
}

if [ "$(printf 'a\n' | GREP_OPTIONS=-v grep -c a || true)" = 0 ]; then
	printf 'test-gate-env: this grep reads GREP_OPTIONS (%s): each case below is a RED for its gate\n' \
		"$(grep --version 2>&1 | head -1)"
else
	printf 'test-gate-env: this grep ignores GREP_OPTIONS (%s): each case below proves only its plant\n' \
		"$(grep --version 2>&1 | head -1)"
fi

# expect <case> <GREP_OPTIONS value> <the gate's message> <command...>: runs
# the command with that value in the environment and requires exit 1 and the
# message.
expect() {
	local name="$1" value="$2" message="$3" status=0
	shift 3
	env GREP_OPTIONS="$value" "$@" </dev/null >"$work/$name.log" 2>&1 || status=$?
	if [ "$status" -ne 1 ]; then
		problem "$name: with GREP_OPTIONS='$value' the gate exited $status, expected 1; its last lines:"
		tail -n 8 "$work/$name.log" | cut -c 1-200 >&2
	elif ! grep -q -F -e "$message" "$work/$name.log"; then
		problem "$name: with GREP_OPTIONS='$value' the gate failed, but not with '$message'; its last lines:"
		tail -n 8 "$work/$name.log" | cut -c 1-200 >&2
	else
		printf 'test-gate-env: ok %-22s GREP_OPTIONS=%-6s %s\n' "$name" "'$value'" "$message"
		passed=$((passed + 1))
	fi
}

# --- check-no-stubs.sh: a `todo!` in production code ----------------------------
#
# The marker greps are `grep -F -- <marker>` over the production lines, with no
# positive control; `-q` empties every one of them.

stubs="$work/stubs"
mkdir -p "$stubs/scripts" "$stubs/docs"
cp -R src "$stubs/src"
cp build.rs "$stubs/build.rs"
cp docs/parity.md "$stubs/docs/parity.md"
cp scripts/check-no-stubs.sh "$stubs/scripts/"
printf 'pub fn planted() {\n    todo!()\n}\n' >"$stubs/src/zz_planted.rs"
expect no-stubs -q "unfinished-work marker 'todo!' in production code" \
	"$stubs/scripts/check-no-stubs.sh"

# --- check-preview-contract.sh: a README without the install command -------
#
# `grep -Fq <command> README.md`; `-v` makes it true of any README with a line
# that is not the command.

preview_tree() {
	local root="$work/$1"
	mkdir -p "$root/.github/workflows"
	cp .github/workflows/*.yml "$root/.github/workflows/"
	cp .github/preview-epoch "$root/.github/preview-epoch"
	cp README.md "$root/README.md"
	printf '%s\n' "$root"
}
readme_tree="$(preview_tree readme)"
grep -v -F -e 'brew install' README.md >"$readme_tree/README.md" || true
expect preview-contract -v "does not document the qualified install command" \
	scripts/check-preview-contract.sh "$readme_tree/.github/workflows/preview.yml" "$readme_tree"

# --- test-preview-contract.sh: a mutant caught for another break -----------
#
# It requires each mutant's failure to name that mutant's own break, by
# `grep -Fq <problem> <log>`; `-v` makes that true of any log with another
# line. So a check whose epoch-gate message changed -- whose `gate-removed`
# mutant fails for some other reason -- would be reported caught.

contract="$work/contract"
mkdir -p "$contract/scripts"
preview_tree contract >/dev/null
cp scripts/test-preview-contract.sh "$contract/scripts/"
ruby -e '
	path, from, to = ARGV
	text = File.read(path)
	abort "#{from.inspect} occurs #{text.scan(from).length} times, expected once" unless text.scan(from).length == 1
	File.write(ARGV[3], text.sub(from) { to })
' scripts/check-preview-contract.sh "preflight has no epoch gate" "preflight has lost its epoch gate" \
	"$contract/scripts/check-preview-contract.sh"
chmod 755 "$contract/scripts/check-preview-contract.sh"
expect test-preview-contract -v "gate-removed failed the check, but not for its own break" \
	"$contract/scripts/test-preview-contract.sh"

# --- the cargo gates: a run in which a test was renamed --------------------
#
# Both read the harness's own count, `test result: ok. N passed`, with
# `grep -qE`; `-v` makes it true of any output with another line. And BSD grep
# with `-v -o` prints every line that does not match, so the preflight gate's
# count of measurement lines is the count of the lines that are not one.

bin="$work/bin"
mkdir -p "$bin"
cat >"$bin/cargo" <<'EOF'
#!/bin/sh
# One test of the two ran: the other was renamed.
printf '   Compiling xfx v0.1.0\n'
printf '    Finished `release` profile [optimized] target(s) in 0.10s\n'
printf '     Running unittests src/lib.rs\n\n'
printf 'running %s test\n' "$XFX_TEST_CARGO_RAN"
i=0
while [ "$i" -lt "$XFX_TEST_CARGO_LINES" ]; do
	printf 'note: line %d of a run\n' "$i"
	i=$((i + 1))
done
printf '\ntest result: ok. %s passed; 0 failed; 0 ignored; 0 measured; 4000 filtered out\n' "$XFX_TEST_CARGO_RAN"
EOF
chmod 755 "$bin/cargo"

cost="$work/cost"
mkdir -p "$cost/scripts"
cp scripts/check-tui-preflight-cost.sh "$cost/scripts/"
expect tui-preflight-cost -v "the filters did not run all 2 cost tests" \
	env PATH="$bin:$PATH" XFX_TEST_CARGO_RAN=1 XFX_TEST_CARGO_LINES=12 \
	"$cost/scripts/check-tui-preflight-cost.sh"

clock="$work/clock"
mkdir -p "$clock/scripts"
cp scripts/check-usage-clock.sh "$clock/scripts/"
expect usage-clock -v "the filter did not run the one clock test" \
	env PATH="$bin:$PATH" XFX_TEST_CARGO_RAN=0 XFX_TEST_CARGO_LINES=0 \
	"$clock/scripts/check-usage-clock.sh"

# --- git's environment and attribute files ---------------------------------

if [ "$(id -u)" -eq 0 ]; then
	problem "running as root, which reads a mode-000 file; the unreadable case would prove nothing"
	exit 1
fi

# The retired product name and a key, assembled: this file is tracked, and the
# gates read it.
retired="f"
retired="${retired}x"
retired="${retired}r"
key_prefix="sk"
key="${key_prefix}-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"

# Fixtures, each with one violation: a tracked path carrying the retired name,
# the retired name inside a tracked Cargo.lock, a key in a tracked file.
ipath="$work/ipath"
mkdir -p "$ipath/scripts" "$ipath/docs"
cp scripts/check-xfx-identity.sh "$ipath/scripts/"
printf 'Clean text.\n' >"$ipath/docs/${retired}rename-notes.md"
sg -C "$ipath" init -q
sg -C "$ipath" add -A

ilock="$work/ilock"
new_ilock() {
	rm -rf "${ilock:?}"
	mkdir -p "$ilock/scripts"
	cp scripts/check-xfx-identity.sh "$ilock/scripts/"
	printf 'name = "%s-core"\n' "$retired" >"$ilock/Cargo.lock"
	sg -C "$ilock" init -q
	sg -C "$ilock" add -A
}
new_ilock

keyed="$work/keyed"
mkdir -p "$keyed/scripts"
cp scripts/check-no-secrets.sh "$keyed/scripts/"
printf 'clean\n' >"$keyed/a.txt"
printf '%s\n' "$key" >"$keyed/b.txt"
sg -C "$keyed" init -q
sg -C "$keyed" add -A

# Indexes without the violation.
GIT_INDEX_FILE="$work/ipath.index" GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
	git -C "$ipath" add scripts/check-xfx-identity.sh
GIT_INDEX_FILE="$work/keyed.index" GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
	git -C "$keyed" add a.txt scripts/check-no-secrets.sh

# Attributes calling a .lock binary; a config naming them; a tree carrying
# them as a .gitattributes, for GIT_ATTR_SOURCE.
printf '*.lock -diff\n' >"$work/attributes"
printf '[core]\n\tattributesFile = %s\n' "$work/attributes" >"$work/attributes.config"
attr_blob="$(printf '*.lock -diff\n' | sg -C "$ilock" hash-object -w --stdin)"
attr_tree="$(printf '100644 blob %s\t.gitattributes\n' "$attr_blob" | sg -C "$ilock" mktree)"

# A HOME whose git config runs programs -- `core.fsmonitor` and the hooks of
# `core.hooksPath` -- each of which leaves its name in "$ran". And the file a
# `GIT_TRACE` would write. Both must stay empty through every case.
ran="$work/ran"
trace="$work/trace"
home="$work/home"
mkdir -p "$ran" "$home/hooks"
for hook in pre-commit commit-msg post-commit post-checkout pre-push; do
	printf '#!/bin/sh\n: >>"%s/%s"\n' "$ran" "$hook" >"$home/hooks/$hook"
	chmod 755 "$home/hooks/$hook"
done
printf '#!/bin/sh\n: >>"%s/fsmonitor"\nexit 1\n' "$ran" >"$home/fsmonitor"
chmod 755 "$home/fsmonitor"
printf '[core]\n\tfsmonitor = %s\n\thooksPath = %s\n' "$home/fsmonitor" "$home/hooks" >"$home/.gitconfig"

# A repository the gates must neither read nor write. Its tracked files are
# "$decoy_tracks" (default d.txt), copied from "$decoy_from" when that is set.
decoy="$work/decoy"
decoy_tracks="d.txt"
decoy_from=""
new_decoy() {
	local name
	rm -rf "${decoy:?}"
	mkdir -p "$decoy"
	sg -C "$decoy" init -q
	for name in $decoy_tracks; do
		mkdir -p "$(dirname "$decoy/$name")"
		if [ -n "$decoy_from" ]; then
			cp "$decoy_from/$name" "$decoy/$name"
		else
			printf 'decoy\n' >"$decoy/$name"
		fi
	done
	# shellcheck disable=SC2086
	sg -C "$decoy" add -- $decoy_tracks
	sg -C "$decoy" -c user.name=decoy -c user.email=decoy@example.invalid \
		-c commit.gpgsign=false commit -q -m decoy
}
decoy_sums() {
	(
		cd "$decoy/.git" || exit 1
		find . -type f -exec cksum {} +
		find . -type d | sed 's/^/dir /'
		find . -type f -perm -u+x | sed 's/^/exec /'
		find . -type l | while IFS= read -r link; do
			printf 'link %s -> %s\n' "$link" "$(readlink "$link")"
		done
	) | sort
}

# Whatever ran from config, or a trace written, since the last call: a problem
# for case $1.
nothing_ran() {
	local what
	what="$(cd "$ran" && ls)"
	if [ -n "$what" ]; then
		problem "$1: a program from config ran: $(printf '%s' "$what" | tr '\n' ' ')"
		rm -f "${ran:?}"/*
		return 1
	fi
	if [ -e "$trace" ]; then
		problem "$1: git wrote the GIT_TRACE file"
		rm -f "${trace:?}"
		return 1
	fi
}

# refuses <case> <message> <command...>: runs the command against a fresh
# decoy and requires exit 1, the message, the decoy unchanged, nothing run
# from config and no trace written.
refuses() {
	local name="$1" message="$2" status=0
	shift 2
	new_decoy
	decoy_sums >"$work/decoy.before"
	"$@" </dev/null >"$work/$name.log" 2>&1 || status=$?
	decoy_sums >"$work/decoy.after"
	if [ "$status" -ne 1 ]; then
		problem "$name: the gate exited $status, expected 1; its last lines:"
		tail -n 6 "$work/$name.log" | cut -c 1-200 >&2
	elif ! grep -q -F -e "$message" "$work/$name.log"; then
		problem "$name: the gate failed, but not with '$message'; its last lines:"
		tail -n 6 "$work/$name.log" | cut -c 1-200 >&2
	elif ! cmp -s "$work/decoy.before" "$work/decoy.after"; then
		problem "$name: the gate wrote into the decoy repository:"
		{ diff "$work/decoy.before" "$work/decoy.after" || true; } | head -n 6 >&2
	elif nothing_ran "$name"; then
		printf 'test-gate-env: ok %-30s %s\n' "$name" "$message"
		passed=$((passed + 1))
	fi
	# What a failed case left, so the next case starts clean.
	rm -f "${ran:?}"/* "${trace:?}"
}

path_found="the retired product name is still in tracked path names"
content_found="the retired product name is still in tracked content"
key_found="possible OpenAI-style key:"
readme_found="does not document the qualified install command"
grep_failed="git grep could not search"
listing_failed="git ls-files could not list"

refuses identity-git-dir "$path_found" \
	env GIT_DIR="$decoy/.git" "$ipath/scripts/check-xfx-identity.sh"
refuses identity-git-dir-work-tree "$path_found" \
	env GIT_DIR="$decoy/.git" GIT_WORK_TREE="$decoy" "$ipath/scripts/check-xfx-identity.sh"
refuses identity-index-file "$path_found" \
	env GIT_INDEX_FILE="$work/ipath.index" "$ipath/scripts/check-xfx-identity.sh"
refuses identity-object-dir "$path_found" \
	env GIT_OBJECT_DIRECTORY="$decoy/.git/objects" "$ipath/scripts/check-xfx-identity.sh"
refuses identity-common-dir "$path_found" \
	env GIT_COMMON_DIR="$decoy/.git" "$ipath/scripts/check-xfx-identity.sh"
refuses identity-trace "$path_found" \
	env GIT_TRACE="$trace" "$ipath/scripts/check-xfx-identity.sh"

refuses no-secrets-index-file "$key_found" \
	env GIT_INDEX_FILE="$work/keyed.index" "$keyed/scripts/check-no-secrets.sh"
refuses no-secrets-object-dir "$key_found" \
	env GIT_OBJECT_DIRECTORY="$decoy/.git/objects" "$keyed/scripts/check-no-secrets.sh"
refuses no-secrets-common-dir "$key_found" \
	env GIT_COMMON_DIR="$decoy/.git" "$keyed/scripts/check-no-secrets.sh"
# A decoy tracking exactly the fixture's clean files: under its GIT_DIR the
# scan would list those, read them in the fixture's tree, and never open b.txt.
decoy_tracks="a.txt scripts/check-no-secrets.sh"
decoy_from="$keyed"
refuses no-secrets-git-dir "$key_found" \
	env GIT_DIR="$decoy/.git" "$keyed/scripts/check-no-secrets.sh"
decoy_tracks="d.txt"
decoy_from=""

preview() {
	env "$@" scripts/check-preview-contract.sh "$readme_tree/.github/workflows/preview.yml" "$readme_tree"
}
refuses preview-git-dir "$readme_found" preview GIT_DIR="$decoy/.git"
refuses preview-git-dir-work-tree "$readme_found" preview GIT_DIR="$decoy/.git" GIT_WORK_TREE="$decoy"
refuses preview-object-dir "$readme_found" preview GIT_OBJECT_DIRECTORY="$decoy/.git/objects"
refuses preview-common-dir "$readme_found" preview GIT_COMMON_DIR="$decoy/.git"

refuses attributes-config-global "$content_found" \
	env GIT_CONFIG_GLOBAL="$work/attributes.config" "$ilock/scripts/check-xfx-identity.sh"
refuses attributes-config-system "$content_found" \
	env GIT_CONFIG_SYSTEM="$work/attributes.config" "$ilock/scripts/check-xfx-identity.sh"
refuses attributes-config-count "$content_found" \
	env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.attributesFile GIT_CONFIG_VALUE_0="$work/attributes" \
	"$ilock/scripts/check-xfx-identity.sh"
refuses attributes-config-parameters "$content_found" \
	env GIT_CONFIG_PARAMETERS="'core.attributesfile'='$work/attributes'" "$ilock/scripts/check-xfx-identity.sh"
refuses attributes-attr-source "$content_found" \
	env GIT_ATTR_SOURCE="$attr_tree" "$ilock/scripts/check-xfx-identity.sh"

# Attribute files a clone holds, set one at a time and taken away after.
cp "$work/attributes" "$ilock/.git/info/attributes"
refuses attributes-info "$content_found" "$ilock/scripts/check-xfx-identity.sh"
rm -f "$ilock/.git/info/attributes"
sg -C "$ilock" config core.attributesFile "$work/attributes"
refuses attributes-local-config "$content_found" "$ilock/scripts/check-xfx-identity.sh"
sg -C "$ilock" config --unset core.attributesFile
cp "$work/attributes" "$ilock/.gitattributes"
refuses attributes-untracked "$content_found" "$ilock/scripts/check-xfx-identity.sh"
rm -f "$ilock/.gitattributes"

# The identity scan reads the names `git grep -l -z` prints; a config that
# colours git grep's output would colour those names, and a coloured name is
# no file.
sg -C "$ilock" config color.grep always
refuses identity-color-grep "$content_found" "$ilock/scripts/check-xfx-identity.sh"
sg -C "$ilock" config --unset color.grep

# Config only the checkout carries, which makes git fail there while the
# controls, in their own repositories, run clean: the scan must fail, not
# come back empty.
for setting in grep.patternType=bogus grep.extendedRegexp=notabool grep.threads=x; do
	new_ilock
	sg -C "$ilock" config "${setting%%=*}" "${setting#*=}"
	refuses "identity-${setting%%=*}" "$grep_failed" "$ilock/scripts/check-xfx-identity.sh"
done
new_ilock
sg -C "$ilock" config core.quotePath notabool
refuses identity-core.quotePath "git cannot open it" "$ilock/scripts/check-xfx-identity.sh"
# An index git cannot read: `rev-parse` opens the repository without it, and
# the listing of the tracked files is what fails.
new_ilock
printf 'DIRC' >"$ilock/.git/index"
refuses identity-index-corrupt "$listing_failed" "$ilock/scripts/check-xfx-identity.sh"
# The same from a HOME config, so the controls' scans fail too: the path
# scan's own failure, not an empty answer.
new_ilock
mkdir -p "$work/quotepath-home"
printf '[core]\n\tquotePath = notabool\n' >"$work/quotepath-home/.gitconfig"
refuses identity-core.quotePath-home "the path scan could not run in its own control" \
	env HOME="$work/quotepath-home" "$ilock/scripts/check-xfx-identity.sh"
# A tracked file git cannot read: `git grep` says `error: failed to stat`
# and exits 1, "no match".
new_ilock
chmod 000 "$ilock/Cargo.lock"
refuses identity-unreadable "$grep_failed" "$ilock/scripts/check-xfx-identity.sh"
chmod 644 "$ilock/Cargo.lock"
# A checkout-local core.worktree naming a clean tree with the same names.
new_ilock
mkdir -p "$work/elsewhere/scripts"
printf 'name = "clean-core"\n' >"$work/elsewhere/Cargo.lock"
cp scripts/check-xfx-identity.sh "$work/elsewhere/scripts/"
sg -C "$ilock" config core.worktree "$work/elsewhere"
refuses identity-core.worktree "$content_found" "$ilock/scripts/check-xfx-identity.sh"
# Index bits that part the working tree from what git grep reads: a
# skip-worktree entry it passes over, and an assume-unchanged one it reads
# from the index while the binary probe reads a working copy with a NUL.
# Each refusal names a remedy that ends in a scan. For a sparse checkout that
# is `git sparse-checkout disable`: clearing skip-worktree on a file the
# working tree lacks leaves a deleted file, which the scan skips.
for bit in skip-worktree assume-unchanged; do
	case "$bit" in
	skip-worktree) message="git sparse-checkout disable" ;;
	assume-unchanged) message="marked assume-unchanged" ;;
	esac
	new_ilock
	sg -C "$ilock" update-index "--$bit" Cargo.lock
	printf 'bin\000ary\n' >"$ilock/Cargo.lock"
	refuses "identity-$bit" "$message" "$ilock/scripts/check-xfx-identity.sh"
done
# A copy with no repository of its own, inside one that tracks it: a vendored
# subtree. The parent tracks vendor/xfx/Cargo.lock with the residue, and this
# script at vendor/xfx/scripts/. Git finds the parent's repository; under
# `--work-tree=.` its paths resolved inside vendor/xfx, where none exists,
# and `git grep` skipped each one without a word.
vendored="$work/vendored"
mkdir -p "$vendored/vendor/xfx/scripts"
cp scripts/check-xfx-identity.sh "$vendored/vendor/xfx/scripts/"
printf 'name = "%s-core"\n' "$retired" >"$vendored/vendor/xfx/Cargo.lock"
sg -C "$vendored" init -q
sg -C "$vendored" add -A
refuses identity-vendored "not a git checkout of its own" "$vendored/vendor/xfx/scripts/check-xfx-identity.sh"
# A linked worktree, whose `.git` is a file: its own checkout, and scanned.
new_ilock
sg -C "$ilock" -c user.name=i -c user.email=i@example.invalid -c commit.gpgsign=false commit -q -m ilock
sg -C "$ilock" worktree add -q "$work/ilock-linked" -b linked
refuses identity-linked-worktree "$content_found" "$work/ilock-linked/scripts/check-xfx-identity.sh"
rm -rf "${work:?}/ilock-linked"
new_ilock

# A HOME config that runs programs: the gates refuse their violations and run
# none of them.
refuses identity-home-programs "$content_found" env HOME="$home" "$ilock/scripts/check-xfx-identity.sh"
refuses no-secrets-home-programs "$key_found" env HOME="$home" "$keyed/scripts/check-no-secrets.sh"
refuses preview-home-programs "$readme_found" preview HOME="$home"

# --- the entry points outside the three gates ------------------------------

# Every workflow step that runs git drops GIT_* before its first git line.
ruby -ryaml -e '
	ARGV.each do |path|
		doc = YAML.safe_load(File.read(path), aliases: true)
		(doc["jobs"] || {}).each do |job, body|
			(body["steps"] || []).each do |step|
				lines = step["run"].to_s.lines.reject { |l| l.strip.start_with?("#") }
				git = lines.index { |l| l =~ /(^|[\s;|&(`])git(\s|$)/ }
				next unless git
				drop = lines.index { |l| l.include?(%q(for name in "${!GIT_@}")) && l.include?(%q(unset "$name")) }
				state = drop && drop < git ? "drops" : "MISSING"
				puts "#{state}\t#{path} #{job}: #{step["name"]}"
			end
		end
	end
' .github/workflows/*.yml >"$work/workflow-steps"
steps="$(wc -l <"$work/workflow-steps" | tr -d ' ')"
if [ "$steps" -eq 0 ]; then
	problem "workflow-steps: no workflow step runs git; the check found nothing to hold"
elif grep -q '^MISSING' "$work/workflow-steps"; then
	problem "workflow-steps: a step runs git before it drops GIT_*:"
	grep '^MISSING' "$work/workflow-steps" | cut -f2 >&2
else
	printf 'test-gate-env: ok %-30s %s\n' workflow-steps "all $steps steps that run git drop GIT_* first"
	passed=$((passed + 1))
fi

# ci.yml's checkout assertion, run as written: under a GIT_DIR naming a decoy
# whose HEAD is the commit it expects, it must still read its own checkout.
ruby -ryaml -e '
	doc = YAML.safe_load(File.read(".github/workflows/ci.yml"), aliases: true)
	step = doc["jobs"].values.flat_map { |j| j["steps"] || [] }.find { |s| s["name"] == "Confirm the checkout is the commit under test" }
	abort "ci.yml has no checkout assertion" unless step
	print step["run"]
' >"$work/assertion.sh"
checkout="$work/checkout"
mkdir -p "$checkout"
sg -C "$checkout" init -q
printf 'checkout\n' >"$checkout/c.txt"
sg -C "$checkout" add c.txt
sg -C "$checkout" -c user.name=c -c user.email=c@example.invalid -c commit.gpgsign=false commit -q -m checkout
assertion() {
	(cd "$checkout" && env COMMIT_UNDER_TEST="$(sg -C "$decoy" rev-parse HEAD)" "$@" bash -e "$work/assertion.sh")
}
refuses ci-checkout-assertion "checked out" assertion GIT_DIR="$decoy/.git" GIT_TRACE="$trace"

# build.rs, compiled alone. build_case <case> <dir> runs it in <dir> under a
# GIT_DIR naming the decoy, a GIT_TRACE and the HOME that runs programs, and
# requires the revision it stamps to be <dir>'s HEAD, the decoy unchanged and
# nothing run. What it printed is left in "$work/<case>.log".
edition="$(sed -n 's/^edition *= *"\([0-9]*\)".*/\1/p' Cargo.toml | head -n 1)"
build_case() {
	local name="$1" dir="$2" want got status=0
	want="$(sg -C "$dir" rev-parse HEAD)"
	new_decoy
	decoy_sums >"$work/decoy.before"
	(cd "$dir" && env -u XFX_BUILD_REVISION -u XFX_BUILD_CHANNEL PROFILE=debug OUT_DIR="$work" \
		GIT_DIR="$decoy/.git" GIT_TRACE="$trace" HOME="$home" "$work/build-script") \
		>"$work/$name.log" 2>&1 </dev/null || status=$?
	decoy_sums >"$work/decoy.after"
	got="$(sed -n 's/^cargo:rustc-env=XFX_BUILD_REVISION=//p' "$work/$name.log")"
	if [ "$status" -ne 0 ]; then
		problem "$name: it exited $status"
		tail -n 6 "$work/$name.log" >&2
		return 1
	elif [ -z "$got" ] || [ "${want#"$got"}" = "$want" ]; then
		problem "$name: it stamped '$got', which is not the HEAD of its checkout, ${want:0:12}"
		return 1
	elif ! cmp -s "$work/decoy.before" "$work/decoy.after"; then
		problem "$name: it wrote into the decoy repository"
		return 1
	elif ! nothing_ran "$name"; then
		return 1
	fi
	stamped="$got"
}
if ! rustc --edition "$edition" build.rs -o "$work/build-script" >"$work/rustc.log" 2>&1; then
	problem "build-script: rustc could not compile build.rs:"
	tail -n 6 "$work/rustc.log" >&2
else
	if build_case build-script "$checkout"; then
		printf 'test-gate-env: ok %-30s %s\n' build-script "stamps the checkout's HEAD ($stamped)"
		passed=$((passed + 1))
	fi
	rm -f "${ran:?}"/* "${trace:?}"
	# A linked worktree of that checkout, on a commit of its own: its `.git`
	# is a file naming the worktree's git directory. The revision is the
	# worktree's, and what build.rs watches for the next commit is the
	# worktree's own HEAD and its branch's ref in the main repository.
	linked="$work/checkout-linked"
	sg -C "$checkout" worktree add -q "$linked" -b linked-build
	printf 'linked\n' >"$linked/l.txt"
	sg -C "$linked" add l.txt
	sg -C "$linked" -c user.name=c -c user.email=c@example.invalid -c commit.gpgsign=false commit -q -m linked
	if build_case build-script-linked-worktree "$linked"; then
		watched="$(sed -n 's/^cargo:rerun-if-changed=//p' "$work/build-script-linked-worktree.log")"
		if ! grep -q -E '(^|/)\.git/worktrees/checkout-linked/HEAD$' <<<"$watched"; then
			problem "build-script-linked-worktree: it does not watch the worktree's own HEAD; it watches:"
			printf '%s\n' "$watched" >&2
		elif ! grep -q -E '(^|/)\.git/refs/heads/linked-build$' <<<"$watched"; then
			problem "build-script-linked-worktree: it does not watch the branch ref in the main repository; it watches:"
			printf '%s\n' "$watched" >&2
		else
			printf 'test-gate-env: ok %-30s %s\n' build-script-linked-worktree "stamps the worktree's HEAD ($stamped), watches its HEAD and ref"
			passed=$((passed + 1))
		fi
	fi
fi
rm -f "${ran:?}"/* "${trace:?}"

# test-check-no-secrets.sh, whole, under a GIT_DIR naming the decoy, a
# GIT_TRACE and the HOME that runs programs: it passes, and none of them
# touches anything. This is CI's only full run of it (ci.yml runs it nowhere
# else), so a failure prints its whole log.
new_decoy
decoy_sums >"$work/decoy.before"
status=0
env GIT_DIR="$decoy/.git" GIT_TRACE="$trace" HOME="$home" ./scripts/test-check-no-secrets.sh \
	>"$work/test-check-no-secrets.log" 2>&1 </dev/null || status=$?
decoy_sums >"$work/decoy.after"
nested=failed
if [ "$status" -ne 0 ]; then
	problem "test-check-no-secrets: it exited $status"
elif ! cmp -s "$work/decoy.before" "$work/decoy.after"; then
	problem "test-check-no-secrets: it wrote into the decoy repository"
elif nothing_ran test-check-no-secrets; then
	nested=ok
	printf 'test-gate-env: ok %-30s %s\n' test-check-no-secrets "$(tail -n 1 "$work/test-check-no-secrets.log" | cut -c 1-60)"
	passed=$((passed + 1))
fi
if [ "$nested" != ok ]; then
	printf 'test-gate-env: the whole log of test-check-no-secrets.sh:\n' >&2
	cat "$work/test-check-no-secrets.log" >&2
fi
rm -f "${ran:?}"/* "${trace:?}"

if [ "$failures" -ne 0 ]; then
	printf 'test-gate-env: %d problem(s), %d case(s) passed\n' "$failures" "$passed" >&2
	exit 1
fi
printf 'test-gate-env: ok (%d cases)\n' "$passed"
