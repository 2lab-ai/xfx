#!/usr/bin/env bash
# Train 16: train 15's unit-tests.sh, its units this train's eleven (each
# with its range and its stack top: a test a later unit of the same line
# renamed or folded is the line's, not the train's) and the join commit's
# three tests; no test is dropped on purpose by a resolution here, so no
# `--gone`. unit_tests.py over the gate's two `cargo test` runs at the code
# head. Output: unit-tests.txt, through redacted.py. Round 2: its two join
# cases too (`round2`, 2b78aab6..6137bd3d).
# usage: unit-tests.sh <repo> <head> <gate-log-dir> <scratch>
set -u
repo=$1 head=$2 logs=$3 T=$4
here=$(cd "$(dirname "$0")" && pwd)
PR=bacb1052 SL=32eb858f
{ echo "# unit_tests.py at $head over the gate's logs (its test and test-fault steps)"
  python3 -I -B "$here/unit_tests.py" "$repo" "$head" "$logs" \
    PROFILE-REFUSE-AT-LAUNCH=3943d667..0a334851@$PR PROFILE-REFUSE-TYPED-KEYS=0a334851..9c64ddd3@$PR \
    PERMISSION-MODE-CASE=9c64ddd3..f74d2fd5@$PR PROFILE-JSON-LIMITS=f74d2fd5..bacb1052 \
    SLASH-LEADING-WS=afd891af..4bb8af77@$SL SLASH-ARG-SKILLS=4bb8af77..b9b11596@$SL \
    SLASH-MIXED-SKILLS=b9b11596..32eb858f \
    TEST-ENV-ISOLATION=32afd307..7c3f6c1e LOCAL-PATHS-WRAP=71d3e02b..dd9f5f0a \
    APPROVAL-PASTE-SPACES=df45025c..ca496c10 META-PREFIX-DECODE=df45025c..6d94fbc0 \
    join=1bce5348..622b2865 round2=2b78aab6..6137bd3d
  echo "# exit $?"; } 2>&1 | python3 -I -B "$here/redacted.py" "$here/unit-tests.txt" "$T=<scratch>" "$repo=<repo>"
