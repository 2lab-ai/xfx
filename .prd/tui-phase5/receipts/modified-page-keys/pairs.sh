#!/usr/bin/env bash
# The scenario's pairs, two ways (run from anywhere; reads this directory).
#
# 1. Across the sides: every fx/ snap against the same snap under <side>/
#    (red or xfx), through the M1 receipt's cells.py (row 1, the banner, left
#    out), and the composer rows each side shows (`┃ ...`).
# 2. Within each side: a hash of rows 2-40 of each snap's text, so a key that
#    moved the full transcript changes the hash and one that did not leaves
#    it. The full-transcript snaps carry each side's own record header times
#    and a session record fx draws taller (the ledger's full-transcript-shift),
#    so across the sides they differ before any key: for them the claim is
#    that a held page key adds no row to that difference, which the hashes and
#    the differing-row lists of `full` and `full-*-ctrl-pgup` say together.
#
# usage: pairs.sh <red|xfx>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
side=${1:?usage: pairs.sh <red|xfx>}
cells=$here/../../../tui-phase4/receipts/m1/cells.py
name=p5-modified-page-keys-40x120
labels=(answered draft full full-ctrl-pgup full-kp-ctrl-pgup full-pgup closed typed)
same=0 total=0
echo "## fx against $side, cell by cell (cells.py, row 1 left out)"
for label in "${labels[@]}"; do
  fxf=$here/fx/fx-$name-$label.ansi
  xf=$here/$side/xfx-$name-$label.ansi
  total=$((total + 1))
  differ=$(python3 -I "$cells" "$fxf" "$xf" 2>&1 | sed -n 's/^row \([0-9]*\): fx .*/\1/p' | tr '\n' ' ' || true)
  verdict=DIFF
  if [[ -z $differ ]]; then verdict=SAME; same=$((same + 1)); fi
  printf '%-4s %-18s rows: %s\n' "$verdict" "$label" "${differ:-none}"
  printf '       fx:  %s\n       %-4s %s\n' "$(grep '┃' "${fxf%.ansi}.txt" | head -1)" "$side:" "$(grep '┃' "${xf%.ansi}.txt" | head -1)"
done
echo "$same of $total pairs identical"
echo
echo "## each side against itself: rows 2-40 of each snap, sha256"
for s in fx "$side"; do
  prefix=$s
  [[ $s == red ]] && prefix=xfx
  for label in "${labels[@]}"; do
    f=$here/$s/$prefix-$name-$label.txt
    printf '%-4s %-18s %s\n' "$s" "$label" "$(sed 1d "$f" | shasum -a 256 | cut -c1-12)"
  done
done
echo
echo "## the composer rows of typed, each side (X is where the caret was)"
for s in fx "$side"; do
  prefix=$s
  [[ $s == red ]] && prefix=xfx
  printf '%-4s %s\n' "$s" "$(grep '┃' "$here/$s/$prefix-$name-typed.txt" | tr '\n' '|')"
done
echo
# The banner is on screen only once the bare PgUp has paged to the head.
echo "banners (full-pgup row 1): fx $(head -1 "$here/fx/fx-$name-full-pgup.txt" | sed 's/ *$//') / $side $(head -1 "$here/$side/xfx-$name-full-pgup.txt" | sed 's/ *$//')"
