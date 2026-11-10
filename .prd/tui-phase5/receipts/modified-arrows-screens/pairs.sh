#!/usr/bin/env bash
# The two scenarios' pairs (run from anywhere; reads this directory).
#
# 1. Across the sides: every fx/ snap against the same snap under <side>/
#    (red or xfx), through the M1 receipt's cells.py (row 1, the banner, left
#    out); for p5-modified-arrows-full the composer rows each side shows
#    (`┃ ...`), for p5-modified-arrows-question the page and the field row.
# 2. Within each side, for p5-modified-arrows-full: a hash of rows 2-40 of
#    each full-transcript snap, so a key that moved the screen changes the
#    hash and one that did not leaves it. Those snaps carry each side's own
#    record times and fx's taller session record and second `· Response`
#    header (the ledger's session-record and fulltranscript-response-header
#    causes), so across the sides they differ before any key; the claim for
#    them is that a held key adds no row to that difference, which the hashes
#    and the differing-row lists of `full` and of each held-key snap say
#    together.
#
# usage: pairs.sh <red|xfx>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
side=${1:?usage: pairs.sh <red|xfx>}
cells=$here/../../../tui-phase4/receipts/m1/cells.py
same=0 total=0
compare() { # <scenario-size> <label>
  local fxf=$here/fx/fx-$1-$2.ansi xf=$here/$side/xfx-$1-$2.ansi differ verdict
  total=$((total + 1))
  differ=$(python3 -I "$cells" "$fxf" "$xf" 2>&1 | sed -n 's/^row \([0-9]*\): fx .*/\1/p' | tr '\n' ' ' || true)
  verdict=DIFF
  if [[ -z $differ ]]; then verdict=SAME; same=$((same + 1)); fi
  printf '%-4s %-18s rows: %s\n' "$verdict" "$2" "${differ:-none}"
}
full=p5-modified-arrows-full-40x120
question=p5-modified-arrows-question-40x120
inline=(answered draft typed typed2 typed3 typed4)
screens=(full full-super-left full-ctrl-up full-ctrl-b full-ctrl-f)
asked=(p1 ctrl-down alt-up super-up shift-super-down shift-alt-up ctrl-shift-down
  super-left shift-left ctrl-b right-keys field-super-up field-ctrl-up
  field-shift-alt-down field-ctrl-f field-super-down field-left)

echo "## $full: fx against $side, cell by cell (cells.py, row 1 left out)"
for label in "${inline[@]}" "${screens[@]}"; do compare "$full" "$label"; done
echo
echo "## $full: the composer rows of each typed snap (Y X W V are where the caret was)"
for s in fx "$side"; do
  prefix=$s
  [[ $s == red ]] && prefix=xfx
  for label in typed typed2 typed3 typed4; do
    printf '%-4s %-7s %s\n' "$s" "$label" "$(grep '┃' "$here/$s/$prefix-$full-$label.txt" | tr '\n' '|')"
  done
done
echo
echo "## $full: each side against itself, rows 2-40 of each full-transcript snap, sha256"
for s in fx "$side"; do
  prefix=$s
  [[ $s == red ]] && prefix=xfx
  for label in "${screens[@]}"; do
    printf '%-4s %-16s %s\n' "$s" "$label" "$(sed 1d "$here/$s/$prefix-$full-$label.txt" | shasum -a 256 | cut -c1-12)"
  done
done
echo
echo "## $question: fx against $side, cell by cell (cells.py, row 1 left out)"
page() { grep -o 'Question [0-9]* of 20' "$1" | head -1 || true; }
field() { grep -o '3) .*' "$1" | head -1 | sed 's/ *$//' || true; }
for label in "${asked[@]}"; do
  compare "$question" "$label"
  printf '       fx:  %s, %s\n       %-4s %s, %s\n' \
    "$(page "$here/fx/fx-$question-$label.txt")" "$(field "$here/fx/fx-$question-$label.txt")" \
    "$side:" "$(page "$here/$side/xfx-$question-$label.txt")" "$(field "$here/$side/xfx-$question-$label.txt")"
done
echo
echo "$same of $total pairs identical"
echo "banners (question p1 row 1; round 2 recaptured this scenario, the full one is round 1's, captures.txt): fx $(head -1 "$here/fx/fx-$question-p1.txt" | sed 's/ *$//') / $side $(head -1 "$here/$side/xfx-$question-p1.txt" | sed 's/ *$//')"
