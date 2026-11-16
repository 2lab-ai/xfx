#!/usr/bin/env bash
# Train 16: train 15's sweep-compare.sh, its units' committed head sweeps
# this train's six with a per-scenario table (PERMISSION-MODE-CASE's
# scoreboard.md; PROFILE-REFUSE-AT-LAUNCH's committed summary.md has none,
# and its 83 scenarios are in this train's sweep, read pair by pair against
# the base). Nothing else changed.
# Train 15 (train 14's sweep-compare.sh, its sweeps this train's: one pair,
# the base rescored with the head's ledger against the head, over
# selection-sweep.txt; the units' own sweeps this train's eleven committed
# scoreboards): the sweeps' files here, through redacted.py -- each sweep's
# scoreboard; the base against the head pair by pair (compare.py) with the
# screens that moved (screen_moves.py), in classes (screen_classes.py), the
# pairs that gained a differing row (gained_rows.py), and compare.py's and
# screen_moves.py's counts tied together (screen_partition.py); the units'
# new scenarios (new_scenarios.py); each unit's own committed sweep against
# the head's (unit_compare.py).
# usage: sweep-compare.sh <repo> <scratch>   (its sw-head, ref-base)
set -u
repo=$1 T=$2
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$T/sw-head=<sweep-head>" "$T/ref-base=<sweep-base>" "$T=<scratch>"; }
P="python3 -I -B $here"
rd "$here/sweep-head.md" < "$T/sw-head/scoreboard.md"
rd "$here/sweep-base.md" < "$T/ref-base/scoreboard.md"
$P/compare.py "$T/ref-base" "$T/sw-head" | rd "$here/compare-base.txt"
echo "compare-base: exit ${PIPESTATUS[0]}"
$P/screen_moves.py "$T/ref-base" "$T/sw-head" | rd "$here/screen-moves-base.txt"
$P/screen_classes.py "$here/screen-moves-base.txt" | rd "$here/screen-classes.txt"
$P/gained_rows.py "$T/ref-base" "$T/sw-head" | rd "$here/gained-rows.txt"
$P/screen_partition.py "$T/ref-base" "$T/sw-head" | rd "$here/screen-partition.txt"
$P/new_scenarios.py "$T/ref-base" "$T/sw-head" | rd "$here/new-scenarios.txt"
U=.prd/tui-phase5/receipts
{ for spec in bacb1052:$U/permission-mode-case/sweep/head/scoreboard.md \
    32eb858f:$U/slash-leading-ws/sweep-head.md 32eb858f:$U/slash-arg-skills/sweep-head.md \
    32eb858f:$U/slash-mixed-skills/sweep-head.md ca496c10:$U/approval-paste-spaces/sweep-head.md \
    6d94fbc0:$U/meta-prefix-decode/sweep-head.md; do
    echo "## $spec"
    $P/unit_compare.py "$repo" "$T/sw-head" board "$spec"
    echo "# exit $?"
  done; } | rd "$here/compare-units.txt"
