#!/usr/bin/env bash
# The unit's gate in the worktree at its head, one suite-lock slot.
# usage: suite-lock.sh gate.sh <worktree> <out-dir>   (TMPDIR short: <tmp>/<slug>)
set -uo pipefail
W=${1:?worktree}; O=${2:?out dir}
rm -rf "$O"; mkdir -p "$O"
cd "$W"
echo "head $(git rev-parse --short HEAD)"
step() { local name=$1; shift; local t0=$(date +%s); "$@" > "$O/$name.log" 2>&1; local rc=$?; echo "$name rc=$rc $(( $(date +%s) - t0 ))s"; }
step fmt cargo fmt --check
step clippy cargo clippy --locked --all-targets -- -D warnings
step clippy-fi cargo clippy --locked --all-targets --features fault-injection -- -D warnings
step test cargo test --locked --no-fail-fast
step test-fi cargo test --locked --features fault-injection --no-fail-fast
step identity scripts/check-xfx-identity.sh
step stubs scripts/check-no-stubs.sh
step secrets scripts/check-no-secrets.sh
step local-paths scripts/check-no-local-paths.sh
