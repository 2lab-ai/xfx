#!/usr/bin/env bash
#
# GATE-GITENV round 2d: the full gate list, one at a time, in the worktree at
# its head: every step ci.yml's gate job runs, in its order, plus the local
# gates it no longer runs on their own (test-check-no-secrets.sh) and the
# fxcap suites (test-sweep.sh, test-capture.sh). Each with its last lines,
# exit status and time. Smoke evidence goes under <out>, and each gate's whole
# output under <out>/logs/<n>.log.
#
#   gates-full.sh <worktree> <out>
#
# Run under <local-path> the machine is shared,
# and its heavy jobs take a slot each.
set -uo pipefail
cd "$1" || exit 1
out_dir=$2
mkdir -p "$out_dir"
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done
unset GREP_OPTIONS
printf 'head %s\n' "$(git rev-parse --short HEAD)"
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
	echo "gates-full.sh: tracked files differ from the head; the gates would not be the head's"
	exit 1
fi
uname -sm; bash --version | head -1; git --version; rustc --version; cargo --version
printf 'TMPDIR=%s\n' "${TMPDIR-unset}"
log="$(mktemp "${TMPDIR:-/tmp}/xfx-gates-full.XXXXXX")"
trap 'rm -f "$log"' EXIT
failed=0
n=0
mkdir -p "$out_dir/logs"
gate() {
	local start status=0
	n=$((n + 1))
	printf '\n== %s\n' "$*"
	printf 'load before: %s\n' "$(uptime | sed 's/.*load averages*: //')"
	start=$(date +%s)
	"$@" </dev/null >"$log" 2>&1 || status=$?
	cp "$log" "$out_dir/logs/$n.log"
	tail -n 12 "$log"
	printf -- '-> exit %d, %d s\n' "$status" $(($(date +%s) - start))
	[ "$status" -eq 0 ] || failed=$((failed + 1))
}
gate cargo fmt --check
gate cargo clippy --locked --all-targets -- -D warnings
gate cargo test --locked --all-targets
gate cargo clippy --locked --all-targets --features fault-injection -- -D warnings
gate cargo test --locked --features fault-injection --test tui
gate cargo test --locked --lib --features fault-injection
gate ./scripts/check-no-stubs.sh
gate ./scripts/check-no-secrets.sh
gate ./scripts/test-gate-env.sh
gate ./scripts/check-xfx-identity.sh
gate ./scripts/check-preview-contract.sh
gate ./scripts/test-preview-contract.sh
gate ./scripts/check-tui-preflight-cost.sh
gate ./scripts/check-usage-clock.sh
gate cargo build --locked --release
gate ./scripts/smoke.sh target/release/xfx "$out_dir/xfx-smoke"
gate cargo build --locked --release --features fault-injection --target-dir target/faulty
gate ./scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx "$out_dir/xfx-smoke-tui"
gate ./scripts/test-check-no-secrets.sh
gate ./scripts/fxcap/test-sweep.sh
gate ./scripts/fxcap/test-capture.sh
gate actionlint -verbose
printf '\n%d gate(s) failed\n' "$failed"
exit "$failed"
