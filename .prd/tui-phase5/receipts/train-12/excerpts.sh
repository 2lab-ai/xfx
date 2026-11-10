#!/usr/bin/env bash
# Train 12: rows-excerpts.txt, through redacted.py -- the capture rows three
# README figures rest on, from the sweeps' and reruns' own files:
#   1. xfx's /stats row (`ansi_bytes`) in p5-panels-sweep-b's stats snap, main's
#      sweep and the head's;
#   2. cells.py over workspace-index's at-plan pair in each of the head's three
#      reruns (which cell each side bolds);
#   3. p5-mcp-menu-new's `runs` row in the head's sweep: capture.sh's `roots:`
#      line (each root's width), and, for row 6 of the server-proj snap on each
#      side, its width in columns and the column of its `…` -- counted here,
#      the row's text not printed (it holds the cut fragment of a path).
# usage: excerpts.sh <scratch> <checkout>
set -u
T=$1 repo=$2
here=$(cd "$(dirname "$0")" && pwd)
{
  echo "## 1. xfx's /stats row, p5-panels-sweep-b stats"
  for s in main head; do
    echo "$s: $(grep -o 'ansi_bytes=[0-9]*' "$T/sw-$s/runs/p5-panels-sweep-b/xfx/xfx-p5-panels-sweep-b-40x120-stats.txt" | head -1)"
  done
  echo; echo "## 2. workspace-index at-plan, the head's reruns, cells.py fx against xfx"
  for i in 1 2 3; do
    d="$T/rr2/head-workspace-index-$i/runs/workspace-index"
    echo "rerun $i:"
    python3 -I "$repo/scripts/fxcap/cells.py" "$d/fx/fx-workspace-index-40x120-at-plan.ansi" "$d/xfx/xfx-workspace-index-40x120-at-plan.ansi" | cut -c1-160 | sed 's/^/    /'
  done
  echo; echo "## 3. p5-mcp-menu-new, the head's sweep"
  echo "fx's roots: line: $(head -1 "$T/sw-head/runs/p5-mcp-menu-new/fx/capture.log")"
  for side in fx xfx; do
    python3 -I -B - "$T/sw-head/runs/p5-mcp-menu-new/$side/$side-p5-mcp-menu-new-40x120-server-proj.txt" "$side" <<'PY'
import sys, unicodedata
row = open(sys.argv[1], encoding="utf-8").read().split("\n")[5]
cols = sum(2 if unicodedata.east_asian_width(c) in "WF" else 1 for c in row)
at = row.find("…")
print(f"{sys.argv[2]}: row 6 starts {row[:12]!r}, {cols} columns, `…` at column {at + 1 if at >= 0 else 'none'}, ends with it: {row.endswith('…')}")
PY
  done
} | python3 -I -B "$here/redacted.py" "$here/rows-excerpts.txt" "$T=<scratch>"
