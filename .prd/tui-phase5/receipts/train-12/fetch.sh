#!/usr/bin/env bash
# Train 12, round 2: where origin/main is. `git fetch origin main` with its
# output discarded (git prints the remote's URL, which receipts never hold),
# its exit kept; then `git ls-remote origin refs/heads/main` (a sha and a
# ref only), `git rev-parse origin/main`, and whether the train's base is it.
# usage: fetch.sh <repo> <base>
set -u
repo=$1 base=$2
git -C "$repo" fetch -q origin main > /dev/null 2>&1
echo "git fetch origin main: exit $?  ($(date -u +%FT%TZ))"
echo "git ls-remote origin refs/heads/main: $(git -C "$repo" ls-remote origin refs/heads/main 2>/dev/null)"
m=$(git -C "$repo" rev-parse origin/main)
echo "origin/main: $m; the base $base is it: $([ "$m" = "$(git -C "$repo" rev-parse "$base")" ] && echo yes || echo no)"
