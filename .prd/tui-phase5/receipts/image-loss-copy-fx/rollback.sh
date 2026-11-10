#!/bin/bash
# IMAGE-LOSS-COPY-FX's rollback, checked on a throwaway clone: every commit after 714f6283 reverted,
# newest first, in one `git revert`; the tree compared with 714f6283's; then the tests of the area the
# unit touched and --test parity on the reverted tree.
#   usage: rollback.sh <xfx repository> <the unit's head> <an empty scratch directory>
set -u
repo=$1 head=$2 work=$3
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE
git clone -q --no-local "$repo" "$work/rollback" || exit 2
cd "$work/rollback" || exit 2
git checkout -q "$head" || exit 2
echo "# the set, oldest first (git log --reverse 714f6283..$head)"
git log --format='%h %s' --reverse 714f6283..HEAD
echo "# git revert --no-edit 714f6283..HEAD (newest first)"
git revert --no-edit 714f6283..HEAD > "$work/revert.log" 2>&1
echo "revert rc=$?, $(grep -c '^\[' "$work/revert.log") revert commits"
echo "# git diff --stat 714f6283 (empty: the tree is 714f6283's)"
git diff --stat 714f6283
echo "files changed against 714f6283: $(git diff --name-only 714f6283 | wc -l | tr -d ' ')"
echo "# cargo test --locked --lib -- session:: tui::worker interactive:: image:: agent::compact"
cargo test --locked --lib -- session:: tui::worker interactive:: image:: agent::compact 2>&1 | grep -E '^test result|FAILED|^error'
echo "cargo rc=${PIPESTATUS[0]}"
echo "# cargo test --locked --test sessions --test parity --test tool_loop"
cargo test --locked --test sessions --test parity --test tool_loop 2>&1 | grep -E '^test result|FAILED|^error'
echo "cargo rc=${PIPESTATUS[0]}"
echo "# cargo test --locked --test tui -- resume image"
cargo test --locked --test tui -- resume image 2>&1 | grep -E '^test result|FAILED|^error'
echo "cargo rc=${PIPESTATUS[0]}"
