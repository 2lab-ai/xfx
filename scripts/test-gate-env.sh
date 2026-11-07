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
# attribute file set, and requires the violation refused and every file
# under the decoy's `.git` unchanged.
#
# Requires git and ruby (check-preview-contract.sh), as the gates do.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# This file's own greps read the gates' output with options of their own,
# and its own gits build scratch repositories and nothing else; the cases
# hand a gate its variables explicitly.
unset GREP_OPTIONS
for name in $(compgen -e); do
	case "$name" in
	GIT_*) unset "$name" ;;
	esac
done

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
git -C "$ipath" init -q
git -C "$ipath" add -A

ilock="$work/ilock"
mkdir -p "$ilock/scripts"
cp scripts/check-xfx-identity.sh "$ilock/scripts/"
printf 'name = "%s-core"\n' "$retired" >"$ilock/Cargo.lock"
git -C "$ilock" init -q
git -C "$ilock" add -A

keyed="$work/keyed"
mkdir -p "$keyed/scripts"
cp scripts/check-no-secrets.sh "$keyed/scripts/"
printf 'clean\n' >"$keyed/a.txt"
printf '%s\n' "$key" >"$keyed/b.txt"
git -C "$keyed" init -q
git -C "$keyed" add -A

# Indexes without the violation.
env GIT_INDEX_FILE="$work/ipath.index" git -C "$ipath" add scripts/check-xfx-identity.sh
env GIT_INDEX_FILE="$work/keyed.index" git -C "$keyed" add a.txt scripts/check-no-secrets.sh

# Attributes calling a .lock binary; a config naming them; a tree carrying
# them as a .gitattributes, for GIT_ATTR_SOURCE.
printf '*.lock -diff\n' >"$work/attributes"
printf '[core]\n\tattributesFile = %s\n' "$work/attributes" >"$work/attributes.config"
attr_blob="$(printf '*.lock -diff\n' | git -C "$ilock" hash-object -w --stdin)"
attr_tree="$(printf '100644 blob %s\t.gitattributes\n' "$attr_blob" | git -C "$ilock" mktree)"

# A repository the gates must neither read nor write.
decoy="$work/decoy"
new_decoy() {
	rm -rf "$decoy"
	mkdir -p "$decoy"
	git -C "$decoy" init -q
	printf 'decoy\n' >"$decoy/d.txt"
	git -C "$decoy" add d.txt
	git -C "$decoy" -c user.name=decoy -c user.email=decoy@example.invalid \
		-c commit.gpgsign=false commit -q -m decoy
}
decoy_sums() {
	(cd "$decoy/.git" && find . -type f -exec cksum {} + | sort)
}

# refuses <case> <message> <command...>: runs the command against a fresh
# decoy and requires exit 1, the message, and the decoy's .git unchanged.
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
	else
		printf 'test-gate-env: ok %-30s %s\n' "$name" "$message"
		passed=$((passed + 1))
	fi
}

path_found="the retired product name is still in tracked path names"
content_found="the retired product name is still in tracked content"
key_found="possible OpenAI-style key:"
readme_found="does not document the qualified install command"

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

refuses no-secrets-index-file "$key_found" \
	env GIT_INDEX_FILE="$work/keyed.index" "$keyed/scripts/check-no-secrets.sh"
refuses no-secrets-object-dir "$key_found" \
	env GIT_OBJECT_DIRECTORY="$decoy/.git/objects" "$keyed/scripts/check-no-secrets.sh"
refuses no-secrets-common-dir "$key_found" \
	env GIT_COMMON_DIR="$decoy/.git" "$keyed/scripts/check-no-secrets.sh"

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
git -C "$ilock" config core.attributesFile "$work/attributes"
refuses attributes-local-config "$content_found" "$ilock/scripts/check-xfx-identity.sh"
git -C "$ilock" config --unset core.attributesFile
cp "$work/attributes" "$ilock/.gitattributes"
refuses attributes-untracked "$content_found" "$ilock/scripts/check-xfx-identity.sh"
rm -f "$ilock/.gitattributes"

# The identity scan reads the names `git grep -l -z` prints; a config that
# colours git grep's output would colour those names, and a coloured name is
# no file.
git -C "$ilock" config color.grep always
refuses identity-color-grep "$content_found" "$ilock/scripts/check-xfx-identity.sh"
git -C "$ilock" config --unset color.grep

if [ "$failures" -ne 0 ]; then
	printf 'test-gate-env: %d problem(s), %d case(s) passed\n' "$failures" "$passed" >&2
	exit 1
fi
printf 'test-gate-env: ok (%d cases)\n' "$passed"
