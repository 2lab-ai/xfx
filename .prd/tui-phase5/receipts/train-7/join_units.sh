#!/usr/bin/env bash
# Join check 1's tests on each unit alone: a git clone --no-local at the
# unit's tip, the tests appended unchanged (apply_join.py), the lib tests
# built (cargo test --no-run), and -- where they build -- run by name.
# Run inside one hold of the suite lock.
# usage: suite-lock.sh join_units.sh <repo> <head> <scratch>
set -u
repo=$1 head=$2 scratch=$3
here=$(cd "$(dirname "$0")" && pwd)
for unit in models-reask=66854557 stop-next-2=f8d625c3; do
  name=${unit%%=*} tip=${unit#*=}
  dir=$scratch/unit-$name
  rm -rf "$dir"
  git clone -q --no-local "$repo" "$dir" && git -C "$dir" checkout -q "$tip"
  echo "== $name at $(git -C "$dir" log --oneline -1 | cut -c1-80)"
  python3 -I -B "$here/apply_join.py" "$repo" "$head" "$dir"
  ( cd "$dir" && cargo test --locked --lib --no-run ) > "$dir.build.log" 2>&1
  rc=$?
  echo "build exit $rc"
  grep -E '^error(\[E[0-9]+\])?:' -A3 "$dir.build.log" | grep -E '^error|-->' | head -40
  grep -E '^error: could not compile' "$dir.build.log"
  if [ "$rc" -eq 0 ]; then
    ( cd "$dir" && cargo test --locked --lib -- a_catalog_answer_read_in_a_stops_window a_stop_in_a_turns_catalog_wait catalog_asks_around_a_stop ) 2>&1 | grep -E '^test |test result'
  fi
  git -C "$dir" checkout -q -- . && echo "clone restored: $([ -z "$(git -C "$dir" status --porcelain)" ] && echo clean || echo dirty)"
done
