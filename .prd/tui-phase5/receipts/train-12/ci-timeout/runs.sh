#!/bin/bash
# The measurements behind this receipt, from the files in this folder. Each
# writes its .txt beside it; none prints a path.
# usage: runs.sh <checkout at 1b15dad9> <checkout at the fix>
#   <checkout at 1b15dad9>  PR #90's second head, the one CI swept (a scratch
#                           clone: `git clone --shared` + `checkout 1b15dad9`)
#   <checkout at the fix>   this branch at the commit the selection is re-read on
set -u
here=$(cd "$(dirname "$0")" && pwd)
old=$1 new=$2
a=$here/artifact
( cd "$old" && ./scripts/fxcap/ci-sweep.sh --select --base df46777e ) > "$here/select.txt"
( cd "$new" && ./scripts/fxcap/ci-sweep.sh --select --base df46777e ) > "$here/select-after.txt"
python3 -I "$here/measure.py" "$old" df46777e "$here/ci-sweep2-step.log" "$a/head/scoreboard.json" \
  "$a/head-retry-1/scoreboard.json" "$here/job-steps.txt" > "$here/measure.txt"
python3 -I "$here/receipts_rule.py" "$old" df46777e "$here/ci-sweep2-step.log" > "$here/receipts-rule.txt"
python3 -I "$here/lookahead.py" "$a/head/scoreboard.json" "$a/head-retry-1/scoreboard.json" \
  "$here/../sweep-main.md" "$here/../sweep-head.md" > "$here/lookahead.txt"
