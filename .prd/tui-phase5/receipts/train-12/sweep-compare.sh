#!/usr/bin/env bash
# Train 12 (train 11's sweep-compare.sh, its references this train's): the
# sweeps' files here, through redacted.py -- each sweep's scoreboard (main's
# rescored with the head's ledger first, `sweep.sh --rescore` on a copy);
# main's against the head pair by pair (compare.py) with the screens that
# moved (screen_moves.py); each unit's own sweep, as its receipt commits it,
# against the head's (unit_compare.py: SWEEP-WALLCLOCK-F's five GREEN runs and
# round 2's five, LOCAL-PATHS's verdicts.tsv after round 7,
# INPUT-ROW-LIMIT's 41x120 scoreboard against this train's 41x120 sweep;
# GATE-ENV-MAINTLOCK-F has no sweep); INPUT-ROW-LIMIT's pairs at 41x120
# (pairs_of.py) and 8x120 (short8.py); and every capture file of the head's sweep read for a
# path of this machine (capture_scan.py).
# usage: sweep-compare.sh <repo> <scratch>   (its sw-head, ref-main, sw-irl41, sw-irl8)
set -u
repo=$1 T=$2
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$T/sw-head=<sweep-head>" "$T/ref-main=<sweep-main>" "$T/sw-irl41=<sweep-irl41>" "$T/sw-irl8=<sweep-irl8>" "$T=<scratch>"; }
P="python3 -I -B $here"
SW=.prd/tui-phase5/receipts/sweep-wallclock
for s in head irl41; do rd "$here/sweep-$s.md" < "$T/sw-$s/scoreboard.md"; done
rd "$here/sweep-main.md" < "$T/ref-main/scoreboard.md"
$P/compare.py "$T/ref-main" "$T/sw-head" | rd "$here/compare-main.txt"
echo "compare-main: exit ${PIPESTATUS[0]}"
$P/screen_moves.py "$T/ref-main" "$T/sw-head" | rd "$here/screen-moves-main.txt"
$P/unit_compare.py "$repo" "$T/sw-head" runs "4bc9ab13:$SW/green-verdicts.txt" "4bc9ab13:$SW/green-r2-verdicts.txt" | rd "$here/compare-unit-sweep-wallclock.txt"
echo "compare-unit-sweep-wallclock: exit ${PIPESTATUS[0]}"
$P/unit_compare.py "$repo" "$T/sw-head" tsv 5609108f:.prd/tui-phase5/receipts/local-paths/verdicts.tsv after | rd "$here/compare-unit-local-paths.txt"
echo "compare-unit-local-paths: exit ${PIPESTATUS[0]}"
$P/unit_compare.py "$repo" "$T/sw-irl41" board 3d3d56c8:.prd/tui-phase5/receipts/input-row-limit/sweep-41x120-head.md | rd "$here/compare-unit-input-row-limit.txt"
echo "compare-unit-input-row-limit: exit ${PIPESTATUS[0]}"
{ echo "## 41x120 (pairs_of.py over the sweep's pairs.json)"; $P/pairs_of.py "$T/sw-irl41" p5-input-row-limit
  echo "## 8x120 (short8.py: the sweep stopped on its inventory, so cells.py over its two captures)"; $P/short8.py "$T/sw-irl8"; } | rd "$here/irl-pairs.txt"
$P/capture_scan.py "$T/sw-head" "${SHORT_TMP:?the sweeps' short scratch directory}/sw-head" model-column-idle model-column-reask model-reask model-parked \
  p5-input-model-fail p5-stop-in-catalog-wait model-floor p5-effort-fork-effort-down p5-input-row-limit p5-input-row-limit-menus p5-input-row-limit-short | rd "$here/capture-scan.txt"
echo "capture-scan: exit ${PIPESTATUS[0]}"
