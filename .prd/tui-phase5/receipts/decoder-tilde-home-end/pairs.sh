#!/usr/bin/env bash
# Every fx/ snap of this receipt against the same snap under <side>/ (red or
# xfx), through the M1 receipt's cells.py (row 1, the banner, left out), and
# the composer row each side shows (`┃ ...`).
#
# usage: pairs.sh <red|xfx>  (run from anywhere; reads this directory)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
side=${1:?usage: pairs.sh <red|xfx>}
cells=$here/../../../tui-phase4/receipts/m1/cells.py
name=p5-decoder-tilde-home-end-40x120
same=0 total=0
for fxf in "$here"/fx/fx-$name-*.ansi; do
  label=${fxf##*/fx-$name-}; label=${label%.ansi}
  xf=$here/$side/xfx-$name-$label.ansi
  total=$((total + 1))
  differ=$(python3 -I "$cells" "$fxf" "$xf" 2>&1 | sed -n 's/^row \([0-9]*\): fx .*/\1/p' | tr '\n' ' ' || true)
  verdict=DIFF
  if [[ -z $differ ]]; then verdict=SAME; same=$((same + 1)); fi
  printf '%-4s %-17s rows: %s\n' "$verdict" "$label" "${differ:-none}"
  printf '       fx:  %s\n       xfx: %s\n' "$(grep '┃' "${fxf%.ansi}.txt" | head -1)" "$(grep '┃' "${xf%.ansi}.txt" | head -1)"
done
echo "banners: fx $(head -1 "$here/fx/fx-$name-typed.txt" | sed 's/ *$//') / $side $(head -1 "$here/$side/xfx-$name-typed.txt" | sed 's/ *$//')"
echo "$same of $total pairs identical"
