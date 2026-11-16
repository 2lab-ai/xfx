#!/usr/bin/env bash
# The three scenarios' pairs (run from anywhere; reads this directory).
#
# 1. Across the sides: every fx/ snap against the same snap under <side>/
#    (red or xfx), through the M1 receipt's cells.py (row 1, the banner, left
#    out), with what each side shows of the keys' effect: the composer's rows
#    (`┃ ...`), `/statusline`'s marked values, the question's page and field
#    (`Pick N?`, `3) ...`), the approval's marked choice (`❯ ...`).
#    An approval pair whose differing rows are exactly those of the scenario's
#    `down` pair -- the snap before any key under test, where the two differ
#    in the grant scope's sentence and the saved session's id -- is SCOPE:
#    the key changed nothing fx's did not (APPROVAL-HORIZONTAL-KEYS' receipt
#    reads its pairs the same way).
# 2. Every `cursor` line's position, fx against <side>.
#
# usage: pairs.sh <red|xfx>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
side=${1:?usage: pairs.sh <red|xfx>}
cells=$here/../../../tui-phase4/receipts/m1/cells.py
same=0 scope=0 total=0 baseline=
shown() { # the rows a key's effect shows on, from the snap's text
  grep -E '┃|^ *(Context|Session|Workspace) |Pick [0-9]+\?|^ *(❯ )?[0-9]+\) |❯ [0-9] ' "$1" | sed 's/ *$//; s/^ *//' | tr '\n' '/' || true
}
marks() { # `/statusline`'s bold values: the one each row has on
  python3 -I -c '
import re, sys
out = []
for raw in open(sys.argv[1], encoding="utf-8", errors="replace"):
    plain = re.sub(r"\x1b\[[0-9;]*m", "", raw).strip()
    if plain.split(" ")[0] in ("Context", "Session", "Workspace"):
        on = re.findall(r"\x1b\[1(?:;|m\x1b\[)38;5;255m(off|on)\x1b", raw)
        out.append(plain.split(" ")[0] + "=" + ",".join(on))
print(" ".join(out))
' "$1"
}
compare() { # <scenario-size> <label>
  local fxf=$here/fx/fx-$1-$2 xf=$here/$side/xfx-$1-$2 differ verdict
  total=$((total + 1))
  differ=$(python3 -I "$cells" "$fxf.ansi" "$xf.ansi" 2>&1 | sed -n 's/^row \([0-9]*\): fx .*/\1/p' | sort -un | tr '\n' ' ' || true)
  verdict=DIFF
  if [[ -z $differ ]]; then verdict=SAME; same=$((same + 1));
  elif [[ $1 == p5-super-arrows-approval-* && $differ == "$baseline" ]]; then verdict=SCOPE; scope=$((scope + 1)); fi
  printf '%-4s %-24s rows: %s\n' "$verdict" "$2" "${differ:-none}"
  printf '       fx:   %s %s\n       %-5s %s %s\n' "$(shown "$fxf.txt")" "$(marks "$fxf.ansi")" \
    "$side:" "$(shown "$xf.txt")" "$(marks "$xf.ansi")"
}
labels() { # <scenario-size>: every snap fx took, in its order on disk
  find "$here/fx" -name "fx-$1-*.ansi" | sed "s|.*/fx-$1-||; s|\.ansi$||" | sort
}
baseline=$(python3 -I "$cells" "$here/fx/fx-p5-super-arrows-approval-40x120-down.ansi" \
  "$here/$side/xfx-p5-super-arrows-approval-40x120-down.ansi" 2>&1 |
  sed -n 's/^row \([0-9]*\): fx .*/\1/p' | sort -un | tr '\n' ' ' || true)
for scenario in p5-super-arrows p5-super-arrows-question p5-super-arrows-approval; do
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
echo "$same of $total pairs identical, $scope more differing only in the approval's scope rows (${baseline:-none})"
echo "banners (draft row 1): fx $(head -1 "$here/fx/fx-p5-super-arrows-40x120-draft.txt" | sed 's/ *$//') / $side $(head -1 "$here/$side/xfx-p5-super-arrows-40x120-draft.txt" | sed 's/ *$//')"
