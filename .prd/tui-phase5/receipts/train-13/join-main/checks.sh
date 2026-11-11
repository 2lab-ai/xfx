#!/usr/bin/env bash
# The join of main's checks (train 13's merge-checks.sh, for one merge), each
# written into this folder through ../redacted.py: the merge redone in a clone
# (reproduce.sh); the merge from both ends (merge_ends.py), the two code units
# its blocks sat in (functions.py), docs/parity.md's rows and the ledger's
# entries (rows_ends.py); the five reference scans and the content check at
# the merge and at the code head, and the content check at both parents; the
# re-point plan and its check; and the join checks (join_checks.sh).
# usage: checks.sh <repo> <scratch>   (no lock: git and python only)
set -u
repo=$1 scratch=$2
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/../redacted.py" "$1" "$repo=<repo>" "$scratch=<scratch>"; }
P="python3 -I -B $here"
T="python3 -I -B $here/.."
BASE=f53606d1 TRAIN=afd891af MAIN=d2b2ca75 MERGE=8adc57df REPOINT=896a11c0 HEAD_C=0dbfaf39
SIDES=(train=$TRAIN main=$MAIN)
CITING=(scripts/fxcap/parity-ledger.toml docs/parity.md scripts/fxcap/capfile.py scripts/smoke-tui.sh src/tui/shell.rs tests/tui.rs tests/sessions.rs)
{ bash "$here/reproduce.sh" "$repo" "$scratch"; echo "# exit $?"; } 2>&1 | rd "$here/reproduce.txt"
{ echo "# merge $MERGE: base $BASE, ours $TRAIN (train 13), theirs $MAIN (main, PR #90)"
  $P/merge_ends.py "$repo" $BASE $TRAIN $MAIN $MERGE; echo "# exit $?"; } 2>&1 | rd "$here/merge-ends.txt"
{ echo "# the code units the two code files' conflict blocks sat in, at the merge $MERGE"
  $P/functions.py "$repo" $BASE $TRAIN $MAIN $MERGE \
    'src/tui/shell.rs:fn a_screen_too_small_for_the_question_cancels_it_rather_than_painting_half() {' \
    'scripts/smoke-tui.sh:def scenario_26(run):'; echo "# exit $?"; } 2>&1 | rd "$here/functions.txt"
{ echo "# merge $MERGE: base $BASE, ours $TRAIN, theirs $MAIN"
  $P/rows_ends.py "$repo" $BASE $TRAIN $MAIN $MERGE; echo "# exit $?"; } 2>&1 | rd "$here/rows-ends.txt"
for s in refs refs_all refs_short refs_bare refs_prd; do
  $T/$s.py "$repo" $MERGE "${SIDES[@]}" 2>&1 | rd "$here/$s-pre.txt"
  $T/$s.py "$repo" $HEAD_C "${SIDES[@]}" 2>&1 | rd "$here/$s-head.txt"
done
for c in $TRAIN $MAIN $MERGE; do
  { for f in "${CITING[@]}"; do echo "## $f"; $T/refs_content.py "$repo" $c "$f"; echo "# exit $?"; done; } 2>&1 | rd "$here/content-$c.txt"
done
{ for f in "${CITING[@]}"; do
    echo "## $f, blame ignoring the re-point $REPOINT (each re-pointed ref read against the frame that wrote its numbers)"
    $T/refs_content.py "$repo" $HEAD_C "$f" --ignore $REPOINT; echo "# exit $?"
    echo "## $f, blame as it is"
    $T/refs_content.py "$repo" $HEAD_C "$f"; echo "# exit $?"
  done; } 2>&1 | rd "$here/content-head.txt"
$P/repoint.py "$repo" --derive 2>&1 | rd "$here/repoint-plan.txt"
{ echo "# at the re-point $REPOINT, whose citing lines are the plan's (the cause after it inserts ledger lines above some)"
  $P/repointed.py "$repo" $REPOINT $REPOINT; echo "# exit $?"; } 2>&1 | rd "$here/repointed.txt"
bash "$here/join_checks.sh" "$repo" $TRAIN $MAIN $HEAD_C 2>&1 | rd "$here/join-checks.txt"
echo "checks: done"
