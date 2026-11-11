#!/usr/bin/env bash
# Train 14 (train 13's sweep-compare.sh, its sweeps this train's: the main
# pair over selection-all.txt and the units' pair over selection-units.txt,
# in place of train 13's extra and round-2 pairs; the units' own sweeps
# this train's nine committed scoreboards): the sweeps' files here, through
# redacted.py -- each sweep's scoreboard (each base's rescored with the
# head's ledger first, rescore.sh); each base against its head pair by pair
# (compare.py) with the screens that moved (screen_moves.py), in classes
# (screen_classes.py), the pairs that gained a differing row
# (gained_rows.py), and compare.py's and screen_moves.py's counts tied
# together (screen_partition.py); the units' new scenarios (new_scenarios.py);
# each unit's own committed sweep against the head's two sweeps taken
# together (unit_compare.py).
# usage: sweep-compare.sh <repo> <scratch>   (its sw-head, ref-base, sw-head-u, ref-base-u)
set -u
repo=$1 T=$2
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$T/sw-head-u=<sweep-head-u>" "$T/ref-base-u=<sweep-base-u>" "$T/sw-head-all=<sweep-head-all>" "$T/sw-head=<sweep-head>" "$T/ref-base=<sweep-base>" "$T=<scratch>"; }
P="python3 -I -B $here"
for x in "" -u; do
  rd "$here/sweep-head$x.md" < "$T/sw-head$x/scoreboard.md"
  rd "$here/sweep-base$x.md" < "$T/ref-base$x/scoreboard.md"
  $P/compare.py "$T/ref-base$x" "$T/sw-head$x" | rd "$here/compare-base$x.txt"
  echo "compare-base$x: exit ${PIPESTATUS[0]}"
  $P/screen_moves.py "$T/ref-base$x" "$T/sw-head$x" | rd "$here/screen-moves-base$x.txt"
  $P/screen_classes.py "$here/screen-moves-base$x.txt" | rd "$here/screen-classes$x.txt"
  $P/gained_rows.py "$T/ref-base$x" "$T/sw-head$x" | rd "$here/gained-rows$x.txt"
done
{ echo "## the main pair"; $P/screen_partition.py "$T/ref-base" "$T/sw-head"
  echo "## the units' pair"; $P/screen_partition.py "$T/ref-base-u" "$T/sw-head-u"; } | rd "$here/screen-partition.txt"
{ echo "## the main pair"; $P/new_scenarios.py "$T/ref-base" "$T/sw-head"
  echo "## the units' pair"; $P/new_scenarios.py "$T/ref-base-u" "$T/sw-head-u"; } | rd "$here/new-scenarios.txt"
# each unit's own sweep, as its receipt commits it, against the head's two
# sweeps taken together (sw-head-all/pairs.json: both pairs.json files, no
# pair in both)
mkdir -p "$T/sw-head-all"
python3 -I -B - "$T/sw-head/pairs.json" "$T/sw-head-u/pairs.json" "$T/sw-head-all/pairs.json" <<'PY'
import json, sys
parts = [json.load(open(f)) for f in sys.argv[1:3]]
keys = [{(p["scenario"], p["label"]) for p in part} for part in parts]
assert not (keys[0] & keys[1])
json.dump(sum(parts, []), open(sys.argv[3], "w"))
PY
U=.prd/tui-phase5/receipts
{ for spec in 4130f4ac:$U/at-folders/sweep-head.md 4130f4ac:$U/at-scope-interim/sweep-head.md \
    4130f4ac:$U/at-scope-interim/sweep-head-r3.md 4130f4ac:$U/at-launch-walk/sweep-head.md \
    4130f4ac:$U/ctrlo-at-keys/sweep-head.md 4130f4ac:$U/at-rank/sweep-head.md \
    e7e1bcf6:$U/ctrlo-other-pickers/sweep-head.md 3ea16930:$U/page-keys-settings-column/sweep-40x120-head.md \
    bde1ddfa:$U/image-loss-copy-fx/sweep-head.md; do
    echo "## $spec"
    $P/unit_compare.py "$repo" "$T/sw-head-all" board "$spec"
    echo "# exit $?"
  done; } | rd "$here/compare-units.txt"
