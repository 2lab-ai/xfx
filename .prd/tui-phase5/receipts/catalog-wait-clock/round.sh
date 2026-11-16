#!/bin/bash
# GREEN stall table at the head (patch applied), then the head's mutants
# (patch reverted), then the two late-stop mutants on the base's clone.
set -u
S="$1"
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE
uptime > "$S/green-head-load.txt"
python3 -I -B "$S/stall_runs.py" "$S/clone" "GREEN, head 5f55cfc8" "$S/green-head.txt" 0 1200 1600 2500 5000
uptime >> "$S/green-head-load.txt"
git -C "$S/clone" checkout -q -- .
python3 -I -B "$S/mutants.py" "$S/clone" head "$S/mutants-head.txt"
python3 -I -B "$S/mutants.py" "$S/clone-base" base "$S/mutants-base-late.txt" retry-wait-stop-late turn-catalog-read-stop-late
echo round-done
