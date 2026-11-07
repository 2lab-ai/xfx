#!/usr/bin/env bash
#
# Proves that no gate takes its grep options from the environment.
#
#   scripts/test-gate-env.sh
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
# Requires git and ruby (check-preview-contract.sh), as the gates do.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# This file's own greps read the gates' output with options of their own.
unset GREP_OPTIONS
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE

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

if [ "$failures" -ne 0 ]; then
	printf 'test-gate-env: %d problem(s), %d case(s) passed\n' "$failures" "$passed" >&2
	exit 1
fi
printf 'test-gate-env: ok (%d cases)\n' "$passed"
