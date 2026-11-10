#!/usr/bin/env bash
# Train 12 (train 10's and 11's check-merge.sh, unchanged but for this line): a merge tree's checks before its commit: cargo check (all targets,
# with and without fault-injection), cargo fmt --check, clippy (both); each exit
# from $? on the line after its command. Run in the train's worktree with the
# merge staged, under the suite lock.
# usage: suite-lock.sh check-merge.sh <worktree> <log-prefix>
set -u
cd "$1" || exit 2
failed=0
for cmd in "cargo check --locked --all-targets" "cargo check --locked --all-targets --features fault-injection" "cargo fmt --check" "cargo clippy --locked --all-targets -- -D warnings" "cargo clippy --locked --all-targets --features fault-injection -- -D warnings"; do
  start=$SECONDS
  $cmd > "$2.$(echo "$cmd" | tr -c 'a-z0-9\n' '_' | cut -c1-60).log" 2>&1
  rc=$?
  echo "$cmd: exit $rc, $((SECONDS - start)) s; $(tail -1 "$2.$(echo "$cmd" | tr -c 'a-z0-9\n' '_' | cut -c1-60).log")"
  [ "$rc" -eq 0 ] || failed=$((failed + 1))
done
echo "# $failed failed"
exit "$failed"
