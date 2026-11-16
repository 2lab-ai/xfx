#!/bin/bash
# Round 3 at the final bound (9d2e7f3c): the head's stall table, then the
# lateness mutants on the head (all round 3's) and the base (the 12 s ones).
set -u
S="$1"   # and WORKTREE in the environment: the unit's worktree
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE
cd "$S/clone" && git checkout -q -- . && git fetch -q "$WORKTREE" fix/p5-catalog-wait-clock && git checkout -q 9d2e7f3c || exit 1
python3 -I -B "$S/stall_patch.py" "$S/clone" > /dev/null || exit 1
git -C "$S/clone" diff > "$S/stall-head3b.patch"
(cd "$S/clone" && cargo test --locked --lib --no-run > "$S/build3b.log" 2>&1) || { echo "build broke"; exit 1; }
uptime > "$S/stall3b-loads.txt"
python3 -I -B "$S/stall_runs.py" "$S/clone" "GREEN, head 9d2e7f3c" "$S/green3b-head.txt" \
  none turn=1200 turn=2500 select=1200 start=1200 turn=1200,select=1200,start=1200 turn=5000 turn=12000
uptime >> "$S/stall3b-loads.txt"
git -C "$S/clone" checkout -q -- .
python3 -I -B "$S/mutants.py" "$S/clone" head "$S/mutants3b-head.txt" \
  first-load-late-turn-read-thread first-load-late-turn-read-thread-6000 first-load-late-turn-read-thread-12000 \
  first-load-late-turn-read-xthread-700 first-load-late-turn-read-xthread-6000 first-load-late-turn-read-xthread-12000 \
  retry-wait-stop-late-thread-500 retry-wait-stop-late-thread-6000 retry-wait-stop-late-thread-12000 \
  retry-wait-stop-late-xthread-500 retry-wait-stop-late-xthread-6000 retry-wait-stop-late-xthread-12000
python3 -I -B "$S/mutants.py" "$S/clone-base" base "$S/mutants3b-base.txt" \
  first-load-late-turn-read-thread-12000 first-load-late-turn-read-xthread-12000 \
  retry-wait-stop-late-thread-12000 retry-wait-stop-late-xthread-12000
echo round3b-done
