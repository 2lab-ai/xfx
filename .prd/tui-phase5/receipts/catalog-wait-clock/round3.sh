#!/bin/bash
# Round 3 in one hold of the suite lock: the stall table on base and head
# (points planted), then the plants reverted and the mutants (head: all;
# base: round 3's).
set -u
S="$1"
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE
python3 -I -B "$S/stall_patch.py" "$S/clone-base" > /dev/null || exit 1
for c in clone-base clone; do
  (cd "$S/$c" && cargo test --locked --lib --no-run > "$S/build3-$c.log" 2>&1) || { echo "build $c broke"; exit 1; }
done
SPECS="none turn=1200 turn=2500 select=1200 start=1200 turn=1200,select=1200,start=1200 turn=5000"
uptime > "$S/stall3-loads.txt"
python3 -I -B "$S/stall_runs.py" "$S/clone-base" "RED, base afd891af" "$S/red3-base.txt" $SPECS
uptime >> "$S/stall3-loads.txt"
python3 -I -B "$S/stall_runs.py" "$S/clone" "GREEN, head ce221384" "$S/green3-head.txt" $SPECS
uptime >> "$S/stall3-loads.txt"
git -C "$S/clone" checkout -q -- .
git -C "$S/clone-base" checkout -q -- .
python3 -I -B "$S/mutants.py" "$S/clone" head "$S/mutants3-head.txt"
python3 -I -B "$S/mutants.py" "$S/clone-base" base "$S/mutants3-base.txt" \
  first-load-late-turn-read-thread-6000 first-load-late-turn-read-xthread-700 \
  first-load-late-turn-read-xthread-6000 retry-wait-stop-late-thread-500 \
  retry-wait-stop-late-thread-6000 retry-wait-stop-late-xthread-500 retry-wait-stop-late-xthread-6000
echo round3-done
