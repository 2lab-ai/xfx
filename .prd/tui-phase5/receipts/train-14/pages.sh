#!/usr/bin/env bash
# Train 14 (new), join check 4 (PAGE-KEYS-SETTINGS-COLUMN x main's
# INPUT-ROW-LIMIT): every line of `src/tui/shell.rs` at <head> that calls
# `input_row_limit` (round 2: every call, the tests' `crate::tui::layout::`
# form too; round 1 read 16 of the 20), with the commit that wrote it (blame)
# and the sides that hold that commit; then functions.py over the function
# the page comes from and the functions PAGE-KEYS wrote page lines in, at
# PAGE-KEYS' base (INPUT-ROW-LIMIT's head), its tip, main, merge 3 and
# <head>. Output: pages.txt, through redacted.py.
# usage: pages.sh <repo> <head>
set -u
repo=$1 head=$2
here=$(cd "$(dirname "$0")" && pwd)
{
  echo "# INPUT-ROW-LIMIT's head and PAGE-KEYS' base: 3d3d56c8; main merged it in $(git -C "$repo" log --format='%h (%s)' --grep='INPUT-ROW-LIMIT (fix/p5-input-row-limit @ 3d3d56c8)' -1 dcfa5146)"
  echo "# each call of input_row_limit at $head: line, the commit that wrote it, the function it is in, and the sides holding that commit"
  tmp=$(mktemp)
  git -C "$repo" show "$head:src/tui/shell.rs" > "$tmp"
  grep -n 'input_row_limit(' "$tmp" | cut -d: -f1 | while read -r n; do
    c=$(git -C "$repo" blame -l -s -L "$n,$n" "$head" -- src/tui/shell.rs | cut -c1-8)
    fn=$(awk -v n="$n" 'NR<=n && /^    (pub(\(crate\))? )?fn [a-z_]+/ {f=$0} NR==n {print f; exit}' "$tmp" | sed -E 's/.*fn ([a-z_]+).*/\1/')
    sides=""
    for s in page-keys=3ea16930 main=dcfa5146 ctrlo-other-pickers=e7e1bcf6 at-stack=4130f4ac; do
      git -C "$repo" merge-base --is-ancestor "$c" "${s#*=}" && sides="$sides ${s%%=*}"
    done
    echo "  :$n $c $fn --$sides"
  done
  rm -f "$tmp"
  echo
  python3 -I -B "$here/functions.py" "$repo" src/tui/layout.rs:input_row_limit:0 src/tui/shell.rs:offer_to_settings \
    src/tui/shell.rs:offer_to_column src/tui/shell.rs:offer_to_mcp_args src/tui/shell.rs:composer_rows -- \
    base-pk=3d3d56c8 page-keys=3ea16930 main=dcfa5146 m3=7a1773d9 head="$head"
  echo "# exit $?"
} 2>&1 | python3 -I -B "$here/redacted.py" "$here/pages.txt" "$repo=<repo>"
