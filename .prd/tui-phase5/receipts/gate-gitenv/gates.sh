#!/usr/bin/env bash
#
# GATE-GITENV's gates, in the worktree, at its head: each with its output (last
# 20 lines), exit status and time.
#
#   gates.sh <worktree> <base rev> [gate...]     (no gate named: all of them)
set -uo pipefail
cd "$1" || exit 1
base=$2
shift 2
unset GREP_OPTIONS
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done
printf 'head %s\n' "$(git rev-parse --short HEAD)"
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
	echo "gates.sh: tracked files differ from the head; the gates would not be the head's"
	exit 1
fi
uname -sm; bash --version | head -1; git --version; rustc --version
printf 'TMPDIR=%s\n' "${TMPDIR-unset}"
printf '\n== what changed since %s\n' "$(git rev-parse --short "$base")"
git diff --stat "$base" HEAD | tail -n 25
out="$(mktemp "${TMPDIR:-/tmp}/xfx-gates.XXXXXX")"
trap 'rm -f "$out"' EXIT
gate() {
	local start status=0
	printf '\n== %s\n' "$*"
	start=$(date +%s)
	"$@" </dev/null >"$out" 2>&1 || status=$?
	tail -n 20 "$out"
	printf -- '-> exit %d, %d s\n' "$status" $(($(date +%s) - start))
}
if [ "$#" -eq 0 ]; then
	set -- fmt build clippy no-secrets no-stubs preview-contract tui-preflight-cost usage-clock identity \
		test-check-no-secrets test-gate-env test-preview-contract test-sweep actionlint
fi
for g in "$@"; do
	case "$g" in
	fmt) gate cargo fmt --check ;;
	build) gate cargo build --locked ;;
	clippy) gate cargo clippy --locked --all-targets -- -D warnings ;;
	no-secrets) gate ./scripts/check-no-secrets.sh ;;
	no-stubs) gate ./scripts/check-no-stubs.sh ;;
	preview-contract) gate ./scripts/check-preview-contract.sh ;;
	tui-preflight-cost) gate ./scripts/check-tui-preflight-cost.sh ;;
	usage-clock) gate ./scripts/check-usage-clock.sh ;;
	identity) gate ./scripts/check-xfx-identity.sh ;;
	test-check-no-secrets) gate ./scripts/test-check-no-secrets.sh ;;
	test-gate-env) gate ./scripts/test-gate-env.sh ;;
	test-preview-contract) gate ./scripts/test-preview-contract.sh ;;
	test-sweep) gate ./scripts/fxcap/test-sweep.sh ;;
	actionlint) gate actionlint -verbose ;;
	esac
done
