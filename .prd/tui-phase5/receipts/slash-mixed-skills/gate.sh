#!/usr/bin/env bash
# The unit's gate on a clean `git clone --no-local` of the code head, one
# suite-lock slot (SLASH-ARG-SKILLS' gate.sh, on a clone).
# usage: suite-lock.sh gate.sh <repo> <rev> <work dir> <out-dir>   (TMPDIR short: <tmp>/<slug>)
set -uo pipefail
repo=${1:?repo}; rev=${2:?rev}; work=${3:?work dir}; O=${4:?out dir}
rm -rf "$O" "$work/clone"; mkdir -p "$O" "$work"
git clone -q --no-local "$repo" "$work/clone" && git -C "$work/clone" checkout -q --detach "$rev" || exit 1
cd "$work/clone"
echo "head $(git rev-parse --short HEAD); porcelain lines: $(git status --porcelain | wc -l | tr -d ' ')"
step() { local name=$1; shift; local t0; t0=$(date +%s); "$@" > "$O/$name.log" 2>&1; local rc=$?; echo "$name rc=$rc $(( $(date +%s) - t0 ))s"; }
step fmt cargo fmt --check
step clippy cargo clippy --locked --all-targets -- -D warnings
step clippy-fi cargo clippy --locked --all-targets --features fault-injection -- -D warnings
step test cargo test --locked --no-fail-fast
step test-fi cargo test --locked --features fault-injection --no-fail-fast
step identity scripts/check-xfx-identity.sh
step stubs scripts/check-no-stubs.sh
step secrets scripts/check-no-secrets.sh
step local-paths scripts/check-no-local-paths.sh
