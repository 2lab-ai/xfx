#!/usr/bin/env bash
# Train 13 (train 12's round-2 fetch.sh, with one line added: whether a commit
# -- train 12's head, this train's base -- is in origin/main): where
# origin/main is. `git fetch origin main` with its output discarded (git
# prints the remote's URL, which receipts never hold), its exit kept; then
# `git ls-remote origin refs/heads/main` (a sha and a ref only), `git
# rev-parse origin/main`, and whether the base is in it.
# usage: fetch.sh <repo> <base>
set -u
repo=$1 base=$2
git -C "$repo" fetch -q origin main > /dev/null 2>&1
echo "git fetch origin main: exit $?  ($(date -u +%FT%TZ))"
echo "git ls-remote origin refs/heads/main: $(git -C "$repo" ls-remote origin refs/heads/main 2>/dev/null)"
m=$(git -C "$repo" rev-parse origin/main)
git -C "$repo" merge-base --is-ancestor "$base" "$m"
in=$?
echo "origin/main: $m; the base $base is in it: $([ "$in" = 0 ] && echo yes || echo no)"
