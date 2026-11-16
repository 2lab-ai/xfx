#!/usr/bin/env bash
# The receipt's own capture pairs, cell by cell (scripts/fxcap/cells.py, the
# M1 receipt's command line: row 1 left out, no ledger normalization), fx's
# against the head's xfx (a57ba760) and the base's (3943d667); then each
# p5-panels-badsettings capture's last row, the only one the head's differs in.
# usage: cells.sh   (run in the repo)
set -u
R=.prd/tui-phase5/receipts/profile-refuse-at-launch
for side in xfx xfx-base; do
  for f in "$R"/fx/*/*.ansi; do
    sc=$(basename "$(dirname "$f")"); lab=${f##*-40x120-}
    echo "== fx/$sc ${lab%.ansi} vs $side/"
    python3 -I scripts/fxcap/cells.py "$f" "$R/$side/$sc/xfx-$sc-40x120-$lab" | cut -c1-200
  done
done
echo "== p5-panels-badsettings launch, row 40 of each (tmux's, masked by the ledger's tmux-dead-pane)"
for side in fx xfx; do
  printf '%s: %s\n' "$side" "$(sed -n 40p "$R/$side/p5-panels-badsettings/$side-p5-panels-badsettings-40x120-launch.txt")"
done
