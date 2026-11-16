#!/usr/bin/env bash
# Train 15 (new), join checks 1 and 4 read in the sweeps: scenarios.py over
# the base (rescored) and the head, for J1's globs (`approval*`,
# `p5-approval*`, `p5-super-arrows-approval`) and J4's scenarios (the two
# re-pointed causes' own, and the unit's split scenario) with the two causes
# named. Output: scenarios.txt, through redacted.py.
# usage: scenarios.sh <scratch>   (its ref-base and sw-head)
set -u
T=$1
here=$(cd "$(dirname "$0")" && pwd)
{ echo "# join check 1: the approval amendment's scenarios"
  python3 -I -B "$here/scenarios.py" "$T/ref-base" "$T/sw-head" 'approval*' 'p5-approval*' 'p5-super-arrows-approval'
  echo
  echo "# join check 4: full-transcript-shift's and session-record-page-bottom's scenarios, and FULLTRANSCRIPT-RESPONSE-HEADER's own"
  python3 -I -B "$here/scenarios.py" "$T/ref-base" "$T/sw-head" --cause full-transcript-shift --cause session-record-page-bottom \
    fulltranscript fulltranscript-notice links read-failures resume-tools timeout \
    p5-transcript-md p5-transcript-md-light p5-modified-page-keys fulltranscript-response-split
} 2>&1 | python3 -I -B "$here/redacted.py" "$here/scenarios.txt" "$T=<scratch>"
