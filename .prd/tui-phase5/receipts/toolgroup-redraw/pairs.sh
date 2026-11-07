#!/usr/bin/env bash
# usage: pairs.sh <fx dir> <xfx dir> <scenario>...
# Each snap of each scenario, fx against xfx: cells.py at 256 colours (row 1,
# the banner, not compared), and for a `-tc` scenario the ramp and kept
# readings of ../inline-code-truecolor/notation.py (a direct grey read as the
# index it spells; kept against the run's first snap). Prints the banner revision of the xfx capture beside it.
W=$(cd "$(dirname "$0")/../../../.." && pwd)
fx=$1; x=$2; shift 2
for s in "$@"; do
  for f in "$fx"/fx-$s-40x120-*.ansi; do
    snap=${f##*-40x120-}; snap=${snap%.ansi}
    xf="$x/xfx-$s-40x120-$snap.ansi"
    banner=$(head -1 "$xf" | grep -o 'v0\.1\.0-[0-9a-f]*')
    if [[ $s == *-tc ]]; then
      # The run's first snap, taken before any theme report: what `kept` reads
      # each later snap's notation against.
      drawn=$(grep -m1 '^snap ' "$W/scripts/fxcap/scenarios/$s.cap" | cut -d' ' -f2)
      out=$(python3 -I "$W/.prd/tui-phase5/receipts/inline-code-truecolor/notation.py" "$W/scripts/fxcap" "$f" "$xf" "$x/xfx-$s-40x120-$drawn.ansi" 2>&1 | grep -E '^(ramp|kept)' | tr '\n' ' ')
      echo "$s $snap [$banner]: $out"
    else
      out=$(python3 -I "$W/scripts/fxcap/cells.py" "$f" "$xf" 2>&1)
      echo "$s $snap [$banner]: $(echo "$out" | head -1)"
      echo "$out" | sed -n '2,40p' | sed 's/^/    /'
    fi
  done
done
