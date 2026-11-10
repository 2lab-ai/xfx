#!/bin/bash
# Round 4 in one hold of the suite lock, at 6073cdc5: the head's stall table
# (points planted; the base's is round 3's red3-base.txt, its code unchanged),
# then the plants reverted and the guard, bound and lateness mutants.
set -u
S="$1"   # and WORKTREE in the environment: the unit's worktree
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE
cd "$S/clone" && git checkout -q -- . && git fetch -q "$WORKTREE" fix/p5-catalog-wait-clock && git checkout -q 6073cdc5 || exit 1
python3 -I -B "$S/stall_patch.py" "$S/clone" > /dev/null || exit 1
git -C "$S/clone" diff > "$S/stall-head4.patch"
(cd "$S/clone" && cargo test --locked --lib --no-run > "$S/build4.log" 2>&1) || { echo "build broke"; exit 1; }
uptime > "$S/stall4-loads.txt"
python3 -I -B "$S/stall_runs.py" "$S/clone" "GREEN, head 6073cdc5" "$S/green4-head.txt" \
  none turn=1200 turn=2500 select=1200 start=1200 turn=1200,select=1200,start=1200 turn=5000 turn=12000
uptime >> "$S/stall4-loads.txt"
git -C "$S/clone" checkout -q -- .
python3 -I -B "$S/mutants.py" "$S/clone" head "$S/mutants4-head.txt" \
  watcher-guard-removed bound-not-asserted \
  first-load-late-turn-read-thread first-load-late-turn-read-thread-12000 \
  first-load-late-turn-read-xthread-700 first-load-late-turn-read-xthread-12000 \
  retry-wait-stop-late-thread-500 retry-wait-stop-late-thread-12000 \
  retry-wait-stop-late-xthread-500 retry-wait-stop-late-xthread-12000
echo round4-done
