#!/usr/bin/env bash
# SLASH-ARG-SKILLS' probe (its probe-mixed.cap, over skills-ws): each fx snap
# against the base's and the head's, through the M1 receipt's cells.py (row 1,
# the banner, left out), and each side's composer, menu header and rows.
# usage: probe.sh  (run from anywhere; reads this directory)
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
cells=$here/../../../tui-phase4/receipts/m1/cells.py
for fxf in "$here"/probe/fx-*.ansi; do
  name=${fxf##*/fx-}
  for side in base head; do
    xf=$here/probe/$side-$name
    if out=$(python3 -I "$cells" "$fxf" "$xf" 2>&1); then
      echo "SAME ${name%.ansi} ($side)"
    else
      echo "DIFF ${name%.ansi} ($side)"
      printf '%s\n' "$out" | head -4 | sed 's/^/    /'
    fi
  done
  for side in fx base head; do
    f=$here/probe/$side-${name%.ansi}.txt
    echo "  $side: $(grep -E '^(Commands|Results) |^  [a-z/]|^┃' "$f" | sed -e 's/[[:space:]]*$//' -e 's/   */  /g' | paste -sd '|' -)"
  done
done
