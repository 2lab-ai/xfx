#!/usr/bin/env bash
# Train 13 (train 12's sweep-compare.sh, its references this train's): the
# sweeps' files here, through redacted.py -- each sweep's scoreboard (the
# base's rescored with the head's ledger first, `sweep.sh --rescore` on a
# copy); the base's against the head pair by pair (compare.py) with the
# screens that moved (screen_moves.py), in classes (screen_classes.py), and
# the pairs that gained a differing row (gained_rows.py).
# The same for the extra selection's sweeps (sw-head-x, ref-base-x).
# usage: sweep-compare.sh <repo> <scratch>   (its sw-head, ref-base, sw-head-x, ref-base-x)
set -u
repo=$1 T=$2
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$T/sw-head-x=<sweep-head-x>" "$T/ref-base-x=<sweep-base-x>" "$T/sw-head=<sweep-head>" "$T/ref-base=<sweep-base>" "$T=<scratch>"; }
P="python3 -I -B $here"
rd "$here/sweep-head.md" < "$T/sw-head/scoreboard.md"
rd "$here/sweep-base.md" < "$T/ref-base/scoreboard.md"
$P/compare.py "$T/ref-base" "$T/sw-head" | rd "$here/compare-base.txt"
echo "compare-base: exit ${PIPESTATUS[0]}"
$P/screen_moves.py "$T/ref-base" "$T/sw-head" | rd "$here/screen-moves-base.txt"
$P/screen_classes.py "$here/screen-moves-base.txt" | rd "$here/screen-classes.txt"
$P/gained_rows.py "$T/ref-base" "$T/sw-head" | rd "$here/gained-rows.txt"
rd "$here/sweep-head-x.md" < "$T/sw-head-x/scoreboard.md"
rd "$here/sweep-base-x.md" < "$T/ref-base-x/scoreboard.md"
$P/compare.py "$T/ref-base-x" "$T/sw-head-x" | rd "$here/compare-base-x.txt"
echo "compare-base-x: exit ${PIPESTATUS[0]}"
$P/screen_moves.py "$T/ref-base-x" "$T/sw-head-x" | rd "$here/screen-moves-base-x.txt"
$P/gained_rows.py "$T/ref-base-x" "$T/sw-head-x" | rd "$here/gained-rows-x.txt"
