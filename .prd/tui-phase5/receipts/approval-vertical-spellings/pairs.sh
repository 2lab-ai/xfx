#!/usr/bin/env bash
# Every fx/ snap of this receipt against the same snap under <side>/ (red or
# xfx), through the M1 receipt's cells.py (row 1, the banner, left out), and
# the choice rows each side shows (the marked one with `❯`).
#
# An edit's question differs before any key, in `ask`: choice 2's words and
# xfx's two rows of what `2` buys, the parity ledger's `approval-grant-scope`
# cause (a pending user decision), and the rows those two push down. So the
# line under each pair is the rows cells.py found, and `floor` says whether
# they are exactly `ask`'s rows at the same side.
#
# usage: pairs.sh <red|xfx>  (run from anywhere; reads this directory)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
side=${1:?usage: pairs.sh <red|xfx>}
cells=$here/../../../tui-phase4/receipts/m1/cells.py
name=p5-approval-vertical-keys-40x120
rows() { python3 -I "$cells" "$1" "$2" 2>&1 | sed -n 's/^row \([0-9]*\): fx .*/\1/p' | tr '\n' ' ' || true; }
choices() { grep -E '^  (❯ |  )[123]  ' "$1" | sed -E 's/^  (❯ |  )([123])  (.{0,22}).*/\1\2 \3/' | tr '\n' '|' || true; }
floor=$(rows "$here/fx/fx-$name-ask.ansi" "$here/$side/xfx-$name-ask.ansi")
at_floor=0 total=0
for fxf in "$here"/fx/fx-$name-*.ansi; do
  label=${fxf##*/fx-$name-}; label=${label%.ansi}
  xf=$here/$side/xfx-$name-$label.ansi
  total=$((total + 1))
  differ=$(rows "$fxf" "$xf")
  verdict=DIFF
  if [[ -z $differ ]]; then verdict=SAME; elif [[ $differ == "$floor" ]]; then verdict=floor; fi
  [[ $verdict == DIFF ]] || at_floor=$((at_floor + 1))
  printf '%-6s %-13s rows: %s\n' "$verdict" "$label" "${differ:-none}"
  printf '         fx:  %s\n         xfx: %s\n' "$(choices "${fxf%.ansi}.txt")" "$(choices "${xf%.ansi}.txt")"
done
echo "floor (ask's rows): $floor"
echo "$at_floor of $total pairs differ in no row but the floor's"
