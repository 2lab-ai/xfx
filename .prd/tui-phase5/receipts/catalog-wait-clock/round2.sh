#!/bin/bash
# Round 2 in one hold of the suite lock: build both planted clones, the
# stall table on base and head, then revert the plants and run the mutants
# (base: the new ones and the two full-lib ones; head: all).
set -u
S="$1"
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE
for c in clone-base clone; do
  (cd "$S/$c" && cargo test --locked --lib --no-run > "$S/build-$c.log" 2>&1) || { echo "build $c failed"; exit 1; }
done
SPECS="none turn=1200 turn=2500 select=1200 start=1200 turn=1200,select=1200,start=1200"
uptime > "$S/stall2-loads.txt"
python3 -I -B "$S/stall_runs.py" "$S/clone-base" "RED, base afd891af" "$S/red2-base.txt" $SPECS
uptime >> "$S/stall2-loads.txt"
python3 -I -B "$S/stall_runs.py" "$S/clone" "GREEN, head a6ab3ee6" "$S/green2-head.txt" $SPECS turn=5000
uptime >> "$S/stall2-loads.txt"
git -C "$S/clone" checkout -q -- .
git -C "$S/clone-base" checkout -q -- .
python3 -I -B "$S/mutants.py" "$S/clone" head "$S/mutants2-head.txt"
python3 -I -B "$S/mutants.py" "$S/clone-base" base "$S/mutants2-base.txt" \
  window-first-load-ignores-stop image-route-read-ignores-stop \
  first-load-late-window-tokio first-load-late-window-thread \
  first-load-late-turn-read-tokio first-load-late-turn-read-thread \
  first-load-late-image-route-tokio first-load-late-image-route-thread
echo round2-done
