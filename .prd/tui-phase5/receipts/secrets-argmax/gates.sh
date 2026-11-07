#!/usr/bin/env bash
#
# SECRETS-ARGMAX's gates, in the worktree, at its head: every gate the receipt
# quotes, each with its output, exit status and time.
#
#   gates.sh <worktree> <base rev>

set -uo pipefail

cd "$1" || exit 1
base=$2
unset GREP_OPTIONS

printf 'head %s\n' "$(git rev-parse --short HEAD)"
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
	echo "gates.sh: tracked files differ from the head; the gates would not be the head's"
	git status --porcelain --untracked-files=no
	exit 1
fi
uname -sm
bash --version | head -1
grep --version 2>&1 | head -1
printf 'TMPDIR=%s\n' "${TMPDIR-unset}"

printf '\n== what changed since %s\n' "$(git rev-parse --short "$base")"
git diff --stat "$base" HEAD | tail -n 25

out="$(mktemp "${TMPDIR:-/tmp}/xfx-gates.XXXXXX")"
trap 'rm -f "$out"' EXIT

# Runs one gate; prints the last 20 lines of what it printed, its exit status
# and its time in whole seconds.
gate() {
	local start status=0
	printf '\n== %s\n' "$*"
	start=$(date +%s)
	"$@" </dev/null >"$out" 2>&1 || status=$?
	tail -n 20 "$out"
	printf -- '-> exit %d, %d s\n' "$status" $(($(date +%s) - start))
}

gate cargo fmt --check
gate ./scripts/check-no-secrets.sh
gate ./scripts/test-check-no-secrets.sh
gate ./scripts/check-xfx-identity.sh
gate ./scripts/check-no-stubs.sh
gate ./scripts/check-preview-contract.sh
gate ./scripts/test-preview-contract.sh
if command -v actionlint >/dev/null 2>&1; then
	gate actionlint .github/workflows/ci.yml
fi
