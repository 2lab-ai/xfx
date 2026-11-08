#!/usr/bin/env bash
# Train 8: train 7's sweep-compare.sh, unchanged but for this line. The sweeps' files here, through redacted.py: each sweep's scoreboard (the
# head's; every reference sweep rescored with the head's ledger first,
# `sweep.sh --rescore` on a copy), pair by pair against the head
# (compare.py), the screens that moved (screen_moves.py), and pass per family
# (families.py).
# usage: sweep-compare.sh <head out> <name>=<reference out>...
set -u
head=$1; shift
here=$(cd "$(dirname "$0")" && pwd)
roots=("$head=<sweep-head>")
for ref in "$@"; do roots+=("${ref#*=}=<sweep-${ref%%=*}>"); done
rd() { python3 -I -B "$here/redacted.py" "$1" "${roots[@]}"; }
rd "$here/sweep-head.md" < "$head/scoreboard.md"
fam=(head="$head")
for ref in "$@"; do
  name=${ref%%=*} out=${ref#*=}
  rd "$here/sweep-$name.md" < "$out/scoreboard.md"
  python3 -I -B "$here/compare.py" "$out" "$head" | rd "$here/compare-$name.txt"
  echo "compare-$name: exit ${PIPESTATUS[0]}"
  python3 -I -B "$here/screen_moves.py" "$out" "$head" | rd "$here/screen-moves-$name.txt"
  fam+=("$name=$out")
done
python3 -I -B "$here/families.py" "${fam[@]}" | rd "$here/families.txt"
