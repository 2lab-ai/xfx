#!/usr/bin/env bash
# The three scenarios' pairs (run from anywhere; reads this directory).
#
# Every fx/ snap against the same snap under <side>/ (red or xfx), through the
# M1 receipt's cells.py (row 1, the banner, left out), with what each side
# shows of the keys' effect: the composer's rows (`┃ ...`), the hint row's
# armed cue (`esc again to ...`), a cancelled line (`Cancelled`), and whether
# the held answer's tail is on the screen (the turn still running shows its
# head only).
#
# usage: pairs.sh <red|xfx>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
side=${1:?usage: pairs.sh <red|xfx>}
cells=$here/../../../tui-phase4/receipts/m1/cells.py
same=0 total=0
shown() { # the rows a key's effect shows on, from the snap's text
  grep -E '┃|esc again to|Cancelled|MARKER-' "$1" | sed 's/ *$//; s/^ *//' | tr '\n' '/' || true
}
compare() { # <scenario-size> <label>
  local fxf=$here/fx/fx-$1-$2 xf=$here/$side/xfx-$1-$2 differ verdict
  total=$((total + 1))
  differ=$(python3 -I "$cells" "$fxf.ansi" "$xf.ansi" 2>&1 | sed -n 's/^row \([0-9]*\): fx .*/\1/p' | sort -un | tr '\n' ' ' || true)
  verdict=DIFF
  if [[ -z $differ ]]; then verdict=SAME; same=$((same + 1)); fi
  printf '%-4s %-22s rows: %s\n' "$verdict" "$2" "${differ:-none}"
  printf '       fx:   %s\n       %-5s %s\n' "$(shown "$fxf.txt")" "$side:" "$(shown "$xf.txt")"
}
labels() { # <scenario-size>: every snap fx took, in its order on disk
  find "$here/fx" -name "fx-$1-*.ansi" | sed "s|.*/fx-$1-||; s|\.ansi$||" | sort
}
for scenario in p5-cancel-double-escape p5-cancel-double-escape-turn p5-modified-meta-prefix; do
  echo "## $scenario-40x120: fx against $side, cell by cell (cells.py, row 1 left out)"
  while read -r label; do compare "$scenario-40x120" "$label"; done < <(labels "$scenario-40x120")
  echo
done
echo "$same of $total pairs identical"
echo "banners (row 1): fx $(head -1 "$here/fx/fx-p5-modified-meta-prefix-40x120-draft.txt" | sed 's/ *$//') / $side $(head -1 "$here/$side/xfx-p5-modified-meta-prefix-40x120-draft.txt" | sed 's/ *$//')"
