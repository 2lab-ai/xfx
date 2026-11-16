#!/usr/bin/env bash
# Train 15 (new), join checks 1 and 3: unit_tests.py over the gate's two
# `cargo test` runs at the code head, every unit with its range and its
# stack top (a test a later unit of the same line renamed or folded is the
# line's, not the train's), and the two tests merge 1 dropped on purpose
# named. Output: unit-tests.txt, through redacted.py.
# usage: unit-tests.sh <repo> <head> <gate-log-dir> <scratch>
set -u
repo=$1 head=$2 logs=$3 T=$4
here=$(cd "$(dirname "$0")" && pwd)
W=src/tui/worker.rs
D=df45025c A=7c9dfb2e
{ echo "# unit_tests.py at $head over the gate's logs (its test and test-fault steps)"
  python3 -I -B "$here/unit_tests.py" "$repo" "$head" "$logs" \
    --gone "$W:a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn=E, deleted by main (IMAGE-LOSS-COPY-FX, train 14) with the history notice it asserts; merge 1 keeps it deleted" \
    --gone "$W:a_stop_through_a_dropped_compaction_tells_it_once_and_leaves_the_history_notice_to_the_turn_that_runs=D's old name; main renamed it a_stop_through_a_dropped_compaction_tells_it_once_before_the_turn_that_runs, which merge 1 keeps, on the unit's harness" \
    CATALOG-WAIT-CLOCK=afd891af..72fc9672 \
    DECODER-TILDE-HOME-END=afd891af..ebb04642@$D TILDE-EMPTY-MODIFIER=ebb04642..fab7a217@$D \
    MODIFIED-PAGE-KEYS=fab7a217..c77a77ce@$D MODIFIED-ARROWS-SCREENS=c77a77ce..3f2d6e78@$D \
    COMPOSER-PARAGRAPH-MOVE=3f2d6e78..28bd1810@$D COMPOSER-SUPER-ARROWS=28bd1810..df45025c \
    FULLTRANSCRIPT-RESPONSE-HEADER=3f2d6e78..fffb3cfe \
    APPROVAL-READINESS-SETTLED=a37a969e..b3a19ede@$A APPROVAL-DRAFT-KEYS=b3a19ede..d7916178@$A \
    APPROVAL-CTRL-J=d7916178..1a0adaec@$A RULE-CONFIRM-KEYS=1a0adaec..7c9dfb2e \
    QUESTION-CTRL-J=afd891af..b5cb6f46 join=07eefa89..0341dd2b
  echo "# exit $?"; } 2>&1 | python3 -I -B "$here/redacted.py" "$here/unit-tests.txt" "$T=<scratch>" "$repo=<repo>"
