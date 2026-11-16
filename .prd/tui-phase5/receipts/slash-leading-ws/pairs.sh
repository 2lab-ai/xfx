#!/usr/bin/env bash
# Every fx/ snap of this receipt against the same snap under <side>/ (red or
# xfx), through the M1 receipt's cells.py (row 1, the banner, left out).
#
# `/version`'s row is each product's version (fx `* version: 0.0.16`, xfx
# `* version: 0.1.0`): the ledger's `product-version` normalization, which
# the sweep applies, is applied here the same way first -- the number after
# `* version: ` read as `<build>` on both sides, in copies under $TMPDIR --
# and every pair it touched is named.
#
# usage: pairs.sh <red|xfx>  (run from anywhere; reads this directory)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
side=${1:?usage: pairs.sh <red|xfx>}
cells=$here/../../../tui-phase4/receipts/m1/cells.py
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sllw-pairs.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
norm() { # <in> <out>: the version number read as <build>
  sed -E 's/(\* version: )[0-9]+\.[0-9]+\.[0-9]+(-[^[:space:][:cntrl:]]+)?/\1<build>/' "$1" > "$2"
}
same=0 total=0
for fxf in "$here"/fx/fx-*.ansi; do
  name=${fxf##*/fx-}
  xf=$here/$side/xfx-$name
  total=$((total + 1))
  norm "$fxf" "$tmp/fx.ansi"
  norm "$xf" "$tmp/xfx.ansi"
  note=''
  cmp -s "$fxf" "$tmp/fx.ansi" && cmp -s "$xf" "$tmp/xfx.ansi" || note='  (version row normalized)'
  if out=$(python3 -I "$cells" "$tmp/fx.ansi" "$tmp/xfx.ansi" 2>&1); then
    same=$((same + 1))
    echo "SAME ${name%.ansi}$note"
  else
    echo "DIFF ${name%.ansi}$note"
    printf '%s\n' "$out" | head -6 | sed 's/^/    /'
  fi
done
echo "$same of $total pairs identical"
