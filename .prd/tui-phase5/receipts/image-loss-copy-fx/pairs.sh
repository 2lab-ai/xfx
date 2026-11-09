#!/bin/bash
# The capture pairs of p5-image-loss-gone, cell by cell (row 1, the banner, not compared), and
# what each run left of the seeded session. Run from the worktree root:
#   bash .prd/tui-phase5/receipts/image-loss-copy-fx/pairs.sh > .prd/tui-phase5/receipts/image-loss-copy-fx/green.txt
set -u
R=.prd/tui-phase5/receipts/image-loss-copy-fx
C=.prd/tui-phase4/receipts/m1/cells.py
for snap in quit.full refused.full; do
  for side in xfx base-xfx; do
    echo "# python3 -I $C fx/fx-p5-image-loss-gone-40x120-$snap.ansi $side/xfx-p5-image-loss-gone-40x120-$snap.ansi"
    python3 -I "$C" "$R/fx/fx-p5-image-loss-gone-40x120-$snap.ansi" "$R/$side/xfx-p5-image-loss-gone-40x120-$snap.ansi"
    echo
  done
done
for side in fx xfx base-xfx; do
  echo "# $side: the last refused frame's text rows, then the session's files after the run"
  sed 's/[[:space:]]*$//' "$R/$side/"*-refused.full.txt | grep -v '^$'
  grep '^=== ~/.x*fx/sessions/' "$R/$side/"*-after.files
  echo
done
echo "# the seeds' sizes (bytes)"
S=scripts/fxcap/scenarios
for f in p5-image-loss-gone-fx.jsonl p5-image-loss-gone-xfx-events.jsonl p5-image-loss-gone-xfx-session.json; do
  echo "$f $(wc -c < "$S/$f" | tr -d ' ')"
done
