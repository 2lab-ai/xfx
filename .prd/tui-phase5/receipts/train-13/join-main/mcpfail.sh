#!/bin/bash
# p5-mcp-fail at the join's code head, three runs of xfx alone (the
# dispatcher's pr90-mcpfail-rerun.sh, its head side): the list's row 6.
set -u
C=$1; O=$2
B=$C/target/release/xfx
for i in 1 2 3; do
  d=$O/head-$i; mkdir -p "$d"
  ( cd "$C" && XFX_BIN=$B scripts/fxcap/capture.sh xfx p5-mcp-fail 40x120 "$d" > "$d/log.txt" 2>&1 ); rc=$?
  f=$(ls "$d"/*list*.txt 2>/dev/null | head -1)
  row=$( [ -n "$f" ] && sed -n 6p "$f" | tr -s ' ' )
  echo "head run $i ($("$B" --version 2>/dev/null)): rc=$rc list row 6:$row"
done
