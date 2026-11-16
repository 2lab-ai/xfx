#!/usr/bin/env bash
# p5-approval-paste-spaces' pairs (run from anywhere; reads this directory).
#
# Every fx/ snap against the same snap under <side>/ (red or xfx), through the
# M1 receipt's cells.py (row 1, the banner, left out), with what each side
# shows: the approval's marked choice and its draft (`❯ ...`), how many times
# the refusal is said, and the end of the draft's last painted row.
#   SAME   no row differs.
#   SCOPE  the rows that differ are exactly those of the scenario's `asked`
#          pair -- the snap before any key under test, where the two differ
#          in the grant scope's sentence and the saved session's id
#          (`approval-grant-scope`); APPROVAL-HORIZONTAL-KEYS' and
#          COMPOSER-SUPER-ARROWS' receipts read their pairs the same way.
#   WINDOW a deny-* pair (a 4093-byte draft) whose two sides say the refusal
#          the same number of times and end the draft's last painted row in
#          the same 12 characters: what differs is how the draft is painted,
#          fx's one row on the caret (approvalDraftWindow) against xfx's three
#          (MAX_DRAFT_ROWS), which moves every row under it (README, Found).
#   DIFF   anything else.
#
# usage: pairs.sh <red|xfx>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
side=${1:?usage: pairs.sh <red|xfx>}
cells=$here/../../../tui-phase4/receipts/m1/cells.py
scenario=p5-approval-paste-spaces-40x120
rows() { # the rows cells.py says differ, fx against <side>
  python3 -I "$cells" "$1" "$2" 2>&1 | sed -n 's/^row \([0-9]*\): fx .*/\1/p' | sort -un | tr '\n' ' ' || true
}
said() { grep -c 'exceeds the input limit and was not applied' "$1" || true; }
choice() { grep -E '❯ [0-9]  ' "$1" | sed 's/ *$//; s/^ *//' | cut -c1-60 || true; }
tail_of_draft() { # the last 12 characters of the last row the draft paints
  python3 -I -c '
import re, sys
lines = [l.rstrip() for l in open(sys.argv[1], encoding="utf-8")]
at = [i for i, l in enumerate(lines) if "❯ " in l and ("Apply once" in l or "Don\x27t apply" in l)]
if not at:
    print("-"); sys.exit()
i = at[-1]
last = lines[i]
for l in lines[i + 1:]:
    if re.match(r"^ {6,}\S", l) and not re.match(r"^ +[0-9]  ", l):
        last = l
    else:
        break
print(repr(last[-12:]))
' "$1"
}
baseline=$(rows "$here/fx/fx-$scenario-asked.ansi" "$here/$side/xfx-$scenario-asked.ansi")
same=0 scope=0 window=0 total=0
echo "## $scenario: fx against $side, cell by cell (cells.py, row 1 left out); scope rows: ${baseline:-none}"
while read -r label; do
  fxf=$here/fx/fx-$scenario-$label xf=$here/$side/xfx-$scenario-$label
  total=$((total + 1))
  differ=$(rows "$fxf.ansi" "$xf.ansi")
  verdict=DIFF
  if [[ -z $differ ]]; then verdict=SAME; same=$((same + 1))
  elif [[ $differ == "$baseline" ]]; then verdict=SCOPE; scope=$((scope + 1))
  elif [[ $label == deny-* && $(said "$fxf.txt") == $(said "$xf.txt") && $(tail_of_draft "$fxf.txt") == $(tail_of_draft "$xf.txt") ]]; then
    verdict=WINDOW; window=$((window + 1))
  fi
  printf '%-6s %-22s rows: %s\n' "$verdict" "$label" "${differ:-none}"
  printf '       fx:    %s | refusals %s | draft ends %s\n' "$(choice "$fxf.txt")" "$(said "$fxf.txt")" "$(tail_of_draft "$fxf.txt")"
  printf '       %-6s %s | refusals %s | draft ends %s\n' "$side:" "$(choice "$xf.txt")" "$(said "$xf.txt")" "$(tail_of_draft "$xf.txt")"
done < <(find "$here/fx" -name "fx-$scenario-*.ansi" | sed "s|.*/fx-$scenario-||; s|\.ansi$||" | sort)
echo
echo "$same of $total pairs identical, $scope more differing only in the scope rows, $window more only in the draft's window"
echo "banners (asked row 1): fx $(head -1 "$here/fx/fx-$scenario-asked.txt" | sed 's/ *$//') / $side $(head -1 "$here/$side/xfx-$scenario-asked.txt" | sed 's/ *$//')"
