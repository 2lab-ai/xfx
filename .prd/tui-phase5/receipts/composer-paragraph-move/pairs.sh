#!/usr/bin/env bash
# The three scenarios' pairs (run from anywhere; reads this directory).
#
# 1. Across the sides: every fx/ snap against the same snap under <side>/
#    (red or xfx), through the M1 receipt's cells.py (row 1, the banner, left
#    out), with the composer rows each side shows (`┃ ...`, the transcript's
#    own `┃ hello` dropped) and, for the menus, the marked row.
# 2. Every `cursor` line's position, fx against <side>.
# 3. Within each side, rows 2-40 of each full-transcript snap of
#    p5-paragraph-move, sha256: a key that moved the screen changes the hash.
#    Those snaps carry each side's own record times and fx's longer session
#    record (the ledger's session-record cause), so across the sides they
#    differ before any key; the claim for them is that a held key moves
#    neither screen, which the hashes say.
#
# usage: pairs.sh <red|xfx>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
side=${1:?usage: pairs.sh <red|xfx>}
cells=$here/../../../tui-phase4/receipts/m1/cells.py
same=0 total=0
composer() { grep '┃' "$1" | grep -v '┃ hello *$' | sed 's/ *$//; s/┃ //; s/┃/·/' | tr '\n' '/' || true; }
marked() { # the first indented row that opens bold white or on the mark's grey
  python3 -I -c '
import re, sys
for raw in open(sys.argv[1], encoding="utf-8", errors="replace"):
    plain = re.sub(r"\x1b\[[0-9;]*m", "", raw).rstrip()
    if plain.startswith("  ") and re.match(r"(?:\x1b\[39m)?\s*\x1b\[(?:1m\x1b\[38;5;255|1;38;5;255|38;5;255m\x1b\[48;5;239)m", raw):
        print("mark: " + plain.strip()[:40]); break
' "$1"
}
compare() { # <scenario-size> <label>
  local fxf=$here/fx/fx-$1-$2 xf=$here/$side/xfx-$1-$2 differ verdict
  total=$((total + 1))
  differ=$(python3 -I "$cells" "$fxf.ansi" "$xf.ansi" 2>&1 | sed -n 's/^row \([0-9]*\): fx .*/\1/p' | sort -un | tr '\n' ' ' || true)
  verdict=DIFF
  if [[ -z $differ ]]; then verdict=SAME; same=$((same + 1)); fi
  printf '%-4s %-22s rows: %s\n' "$verdict" "$2" "${differ:-none}"
  printf '       fx:   %s %s\n       %-5s %s %s\n' "$(composer "$fxf.txt")" "$(marked "$fxf.ansi")" \
    "$side:" "$(composer "$xf.txt")" "$(marked "$xf.ansi")"
}
labels() { # <scenario-size>: every snap fx took, in its order on disk
  find "$here/fx" -name "fx-$1-*.ansi" | sed "s|.*/fx-$1-||; s|\.ansi$||" | sort
}
for scenario in p5-paragraph-move p5-paragraph-move-menus p5-paragraph-move-column; do
  echo "## $scenario-40x120: fx against $side, cell by cell (cells.py, row 1 left out)"
  while read -r label; do compare "$scenario-40x120" "$label"; done < <(labels "$scenario-40x120")
  echo
done
echo "## the cursor lines, fx against $side (x,y)"
for f in "$here"/fx/*.cursor; do
  name=${f##*/fx-}
  printf '%-40s fx %s  %s %s\n' "${name%.cursor}" "$(cat "$f")" "$side" "$(cat "$here/$side/xfx-$name")"
done
echo
echo "## p5-paragraph-move: each side against itself, rows 2-40 of each full-transcript snap, sha256"
for s in fx "$side"; do
  prefix=$s
  [[ $s == red ]] && prefix=xfx
  for label in full full-alt-down full-super-up full-shift-up; do
    printf '%-4s %-14s %s\n' "$s" "$label" "$(sed 1d "$here/$s/$prefix-p5-paragraph-move-40x120-$label.txt" | shasum -a 256 | cut -c1-12)"
  done
done
echo
echo "$same of $total pairs identical"
echo "banners (draft row 1): fx $(head -1 "$here/fx/fx-p5-paragraph-move-40x120-draft.txt" | sed 's/ *$//') / $side $(head -1 "$here/$side/xfx-p5-paragraph-move-40x120-draft.txt" | sed 's/ *$//')"
