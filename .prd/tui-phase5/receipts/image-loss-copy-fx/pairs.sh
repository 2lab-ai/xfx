#!/bin/bash
# The capture pairs of p5-image-loss-gone, -whole, -changed and -folder, cell by cell (row 1, the banner,
# not compared), fx against the head and against the base, and what each run left of the seeded
# session. Run from the worktree root:
#   bash .prd/tui-phase5/receipts/image-loss-copy-fx/pairs.sh > .prd/tui-phase5/receipts/image-loss-copy-fx/green.txt
set -u
R=.prd/tui-phase5/receipts/image-loss-copy-fx
C=.prd/tui-phase4/receipts/m1/cells.py
for snap in gone-40x120-quit.full gone-40x120-refused.full whole-40x120-resumed changed-40x120-resumed changed-40x120-failed folder-40x120-refused.full; do
  for side in xfx base-xfx; do
    echo "# python3 -I $C fx/fx-p5-image-loss-$snap.ansi $side/xfx-p5-image-loss-$snap.ansi"
    python3 -I "$C" "$R/fx/fx-p5-image-loss-$snap.ansi" "$R/$side/xfx-p5-image-loss-$snap.ansi"
    echo
  done
done
for snap in gone-40x120-refused.full whole-40x120-resumed changed-40x120-failed folder-40x120-refused.full; do
  for side in fx xfx base-xfx; do
    w=xfx; [[ $side == fx ]] && w=fx
    echo "# $side, $snap: the frame's text rows, the last 7"
    sed 's/[[:space:]]*$//' "$R/$side/$w-p5-image-loss-$snap.txt" | grep -v '^$' | tail -7
    echo
  done
done
for side in fx xfx base-xfx; do
  w=xfx; [[ $side == fx ]] && w=fx
  for f in gone-40x120-after changed-40x120-resumed changed-40x120-failed folder-40x120-after; do
    [[ -f $R/$side/$w-p5-image-loss-$f.files ]] || continue
    echo "# $side, $f: the session's files"
    grep '^=== ~/.x*fx/sessions/' "$R/$side/$w-p5-image-loss-$f.files"
    echo
  done
done
echo "# the seeds' sizes (bytes)"
S=scripts/fxcap/scenarios
for f in p5-image-loss-gone-fx.jsonl p5-image-loss-gone-xfx-events.jsonl p5-image-loss-gone-xfx-session.json p5-image-loss-changed.bin; do
  echo "$f $(wc -c < "$S/$f" | tr -d ' ')"
done
