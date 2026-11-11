#!/usr/bin/env bash
# Train 14 (new; train 13 kept its selection's command in its README): the
# two sweep selections, as the sweeps read them (copied here as
# selection-all.txt and selection-units.txt), and each made again from this
# folder's scripts at this revision, the scenario names compared (a comment
# line's wording may have moved since; a name may not).
#   selection-all.txt   = selection.py over the merges and the first ledger
#                         commit, then its --extra (the 04e4491d re-point's
#                         entries' scenarios)
#   selection-units.txt = selection_units.py: the scenarios the units' own
#                         committed head sweeps took that selection-all.txt
#                         does not (AT-RANK-PERF committed no sweep); made
#                         when the head was 3a2830fb, which has the same
#                         scenarios as the code head b3dffdad
# Output: selections.txt, through redacted.py.
# usage: selections.sh <repo> <scratch>   (<scratch>/selection-all.txt and selection-units.txt, as swept)
set -u
repo=$1 T=$2
here=$(cd "$(dirname "$0")" && pwd)
U=.prd/tui-phase5/receipts
P="python3 -I -B $here"
cp "$T/selection-all.txt" "$here/selection-all.txt"
cp "$T/selection-units.txt" "$here/selection-units.txt"
names() { grep -v '^#' "$1" | grep -v '^$' | sort; }
{
  echo "# the two selections made again here, their scenario names against the swept files'"
  { $P/selection.py "$repo" 04e4491d d548c078 ecad8518 7a1773d9 fb6dfdb8 8f622949
    $P/selection.py --extra "$repo" 04e4491d d548c078 ecad8518 7a1773d9 fb6dfdb8 8f622949; } > "$T/selection-all.redo" 2>&1
  echo "selection-all.txt: $(names "$T/selection-all.txt" | wc -l | tr -d ' ') names swept; made again $(names "$T/selection-all.redo" | wc -l | tr -d ' '); $(cmp -s <(names "$T/selection-all.txt") <(names "$T/selection-all.redo") && echo 'the same names' || echo 'NAMES DIFFER')"
  diff "$T/selection-all.txt" "$T/selection-all.redo" | sed 's/^/    /'
  $P/selection_units.py "$repo" 3a2830fb dcfa5146 "$T/selection-all.txt" \
    4130f4ac:$U/at-folders/sweep-head.md 4130f4ac:$U/at-scope-interim/sweep-head.md \
    4130f4ac:$U/at-scope-interim/sweep-head-r3.md 4130f4ac:$U/at-launch-walk/sweep-head.md \
    4130f4ac:$U/ctrlo-at-keys/sweep-head.md 4130f4ac:$U/at-rank/sweep-head.md \
    e7e1bcf6:$U/ctrlo-other-pickers/sweep-head.md 3ea16930:$U/page-keys-settings-column/sweep-40x120-head.md \
    bde1ddfa:$U/image-loss-copy-fx/sweep-head.md > "$T/selection-units.redo" 2>&1
  echo "selection-units.txt: $(names "$T/selection-units.txt" | wc -l | tr -d ' ') names swept; made again $(names "$T/selection-units.redo" | wc -l | tr -d ' '); $(cmp -s "$T/selection-units.txt" "$T/selection-units.redo" && echo 'the same file, byte for byte' || echo 'DIFFERENT')"
  echo "no name in both: $(comm -12 <(names "$T/selection-all.txt") <(names "$T/selection-units.txt") | wc -l | tr -d ' ')"
} 2>&1 | python3 -I -B "$here/redacted.py" "$here/selections.txt" "$repo=<repo>" "$T=<scratch>"
