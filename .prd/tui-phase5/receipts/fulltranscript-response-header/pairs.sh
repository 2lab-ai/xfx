#!/usr/bin/env bash
# The scenario's pairs (run from anywhere; reads this directory): every fx/
# snap of `fulltranscript-response-split` against the same snap under <side>/
# (red: the base's binary, xfx: the head's), cell by cell through the M1
# receipt's cells.py, row 1 (the banner) left out. Each side's record headers
# carry its own run's moments, which cells.py does not mask, so the pairs are
# compared twice: as captured, and with every header's
# `YYYY-MM-DD HH:MM:SS.mmm` written `XXXX-XX-XX XX:XX:XX.XXX` on both sides
# (the ledger's `transcript-timestamp` normalization, by hand). Then each
# full snap's header kinds, top to bottom, on both sides.
#
# usage: pairs.sh <red|xfx>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
side=${1:?usage: pairs.sh <red|xfx>}
cells=$here/../../../tui-phase4/receipts/m1/cells.py
name=fulltranscript-response-split-40x120
labels=(launch long table full-table rule full-rule block-only full-block-only
  block-last full-block-last block-first full-block-first blocks full-blocks)
masked=$(mktemp -d "${TMPDIR:-/tmp}/pairs.XXXXXX")
trap 'rm -rf "$masked"' EXIT
mask() {
  python3 -I - "$1" "$2" <<'PY'
import re, sys
data = open(sys.argv[1], "rb").read()
data = re.sub(rb"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}(?= UTC \xc2\xb7 )", b"XXXX-XX-XX XX:XX:XX.XXX", data)
open(sys.argv[2], "wb").write(data)
PY
}
differing() {
  python3 -I "$cells" "$1" "$2" 2>&1 | sed -n 's/^row \([0-9]*\): fx .*/\1/p' | tr '\n' ' ' || true
}
same=0 same_masked=0 total=0
echo "## fx against $side, cell by cell (cells.py, row 1 left out): as captured | moments masked"
for label in "${labels[@]}"; do
  fxf=$here/fx/fx-$name-$label.ansi
  xf=$here/$side/xfx-$name-$label.ansi
  total=$((total + 1))
  raw=$(differing "$fxf" "$xf")
  mask "$fxf" "$masked/fx.ansi"
  mask "$xf" "$masked/x.ansi"
  m=$(differing "$masked/fx.ansi" "$masked/x.ansi")
  verdict=DIFF masked_verdict=DIFF
  if [[ -z $raw ]]; then verdict=SAME; same=$((same + 1)); fi
  if [[ -z $m ]]; then masked_verdict=SAME; same_masked=$((same_masked + 1)); fi
  printf '%-17s captured %s rows: %s | masked %s rows: %s\n' "$label" "$verdict" "${raw:-none}" "$masked_verdict" "${m:-none}"
done
echo "$same of $total pairs identical as captured; $same_masked of $total with the moments masked"
echo
echo "## each full snap's header kinds, top to bottom"
for label in "${labels[@]}"; do
  [[ $label == full-* ]] || continue
  for s in fx "$side"; do
    prefix=$s
    [[ $s == red ]] && prefix=xfx
    printf '%-4s %-17s %s\n' "$s" "$label" "$(sed -n 's/.* UTC · //p' "$here/$s/$prefix-$name-$label.txt" | tr '\n' ' ')"
  done
done
echo
echo "banners (launch row 1): fx $(head -1 "$here/fx/fx-$name-launch.txt" | sed 's/ *$//') / $side $(head -1 "$here/$side/xfx-$name-launch.txt" | sed 's/ *$//')"
