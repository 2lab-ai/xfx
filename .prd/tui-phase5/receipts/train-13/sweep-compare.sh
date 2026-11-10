#!/usr/bin/env bash
# Train 13 (train 12's sweep-compare.sh, its references this train's): the
# sweeps' files here, through redacted.py -- each sweep's scoreboard (the
# base's rescored with the head's ledger first, `sweep.sh --rescore` on a
# copy); the base's against the head pair by pair (compare.py) with the
# screens that moved (screen_moves.py), in classes (screen_classes.py), and
# the pairs that gained a differing row (gained_rows.py).
# usage: sweep-compare.sh <repo> <scratch>   (its sw-head, ref-base)
set -u
repo=$1 T=$2
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$T/sw-head=<sweep-head>" "$T/ref-base=<sweep-base>" "$T=<scratch>"; }
P="python3 -I -B $here"
rd "$here/sweep-head.md" < "$T/sw-head/scoreboard.md"
rd "$here/sweep-base.md" < "$T/ref-base/scoreboard.md"
$P/compare.py "$T/ref-base" "$T/sw-head" | rd "$here/compare-base.txt"
echo "compare-base: exit ${PIPESTATUS[0]}"
$P/screen_moves.py "$T/ref-base" "$T/sw-head" | rd "$here/screen-moves-base.txt"
$P/screen_classes.py "$here/screen-moves-base.txt" | rd "$here/screen-classes.txt"
$P/gained_rows.py "$T/ref-base" "$T/sw-head" | rd "$here/gained-rows.txt"
