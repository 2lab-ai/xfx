#!/usr/bin/env bash
# Every fx/ snap of this receipt against the same snap under <side>/ (red or
# xfx), through the M1 receipt's cells.py (row 1, the banner, left out), and
# the page each side shows ("Question N of 20" off the hint row).
#
# usage: pairs.sh <red|xfx>     (run from anywhere; reads this directory)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
side=${1:?usage: pairs.sh <red|xfx>}
cells=$here/../../../tui-phase4/receipts/m1/cells.py
same=0 total=0
page() { grep -o 'Question [0-9]* of 20' "$1" | head -1 || true; }
for fxf in "$here"/fx/fx-*.ansi; do
  name=${fxf##*/fx-}
  xf=$here/$side/xfx-$name
  total=$((total + 1))
  fxpage=$(page "${fxf%.ansi}.txt")
  xpage=$(page "${xf%.ansi}.txt")
  if out=$(python3 -I "$cells" "$fxf" "$xf" 2>&1); then
    same=$((same + 1))
    echo "SAME ${name%.ansi}  (fx: $fxpage; xfx: $xpage)"
  else
    echo "DIFF ${name%.ansi}  (fx: $fxpage; xfx: $xpage)"
    printf '%s\n' "$out" | head -6 | sed 's/^/    /'
  fi
done
echo "$same of $total pairs identical"
