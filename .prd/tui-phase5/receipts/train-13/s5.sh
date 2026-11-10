#!/usr/bin/env bash
# Train 13 round 2 (S5): the fixed join test at <commit> in a `git clone
# --no-local`: cargo fmt --check, clippy with and without fault-injection,
# and the test alone; each exit from $? on the line after its command. Not
# the gate (that runs at the joined tree): the checks the one changed test
# module needs. No lock: none is a heavy job by the P5 common brief.
# usage: s5.sh <repo> <commit> <scratch>
set -u
repo=$1 c=$2 dir=$3
rm -rf "$dir/s5" && git clone -q --no-local "$repo" "$dir/s5" && git -C "$dir/s5" checkout -q --detach "$c" || exit 2
cd "$dir/s5" || exit 2
echo "# at $(git rev-parse --short=8 HEAD); src changed since e1e15efd: $(git diff --stat e1e15efd HEAD -- src | tail -1)"
for cmd in "cargo fmt --check" "cargo clippy --locked --all-targets -- -D warnings" "cargo clippy --locked --all-targets --features fault-injection -- -D warnings" \
    "cargo test --locked --lib -- --exact tui::worker::tests::a_resumed_sealed_step_that_asked_replays_its_batch_and_says_nothing_of_its_reasoning"; do
  $cmd > "$dir/s5.log" 2>&1
  rc=$?
  echo "$cmd: exit $rc; $(grep -E '^test result|Finished' "$dir/s5.log" | tail -1)"
done
cd / && rm -rf "$dir/s5"
