#!/usr/bin/env bash
# Train 14's join of main (new; train 14's merge-checks.sh for the join):
# the join 583cc13a read from both ends -- the token check (train 7's
# token_check.py) with its self-test, token_control.py and word_control.py
# (the self-test's "a word changed" case plants nothing in an entry with no
# `owner`); the content check over every reference at the join and at
# <head>; the five reference scans at <head> against main 3570a182 and the
# train's 08066ac9; and the re-point 3e073a96 read numbers only. Each
# output written here through redacted.py.
# usage: merge-checks.sh <repo> <head>
set -u
repo=$1 head=$2
here=$(cd "$(dirname "$0")" && pwd)
T=$here/..
rd() { python3 -I -B "$T/redacted.py" "$1" "$repo=<repo>"; }
P="python3 -I -B $T"
{ echo "# token_check.py (train 7's, byte for byte: $(cmp -s "$T/token_check.py" "$T/../train-7/token_check.py" && echo the same file || echo DIFFERENT)) over the join"
  $P/token_check.py "$repo" 583cc13a; echo "# exit $?"
  $P/token_check.py "$repo" --self-test 583cc13a; echo "# exit $?"
  echo "# token_control.py"; $P/token_control.py "$repo" 583cc13a; echo "# exit $?"
  echo "# word_control.py: the self-test's missed case, planted in \`why\`"; python3 -I -B "$here/word_control.py" "$repo" 583cc13a; echo "# exit $?"
} 2>&1 | rd "$here/token-check.txt"
for c in 583cc13a "$head"; do
  { echo "# content.sh at $c"; bash "$T/content.sh" "$repo" "$c"; } 2>&1 | rd "$here/content-$c.txt"
done
for s in refs refs_all refs_short refs_bare refs_prd; do
  $P/$s.py "$repo" "$head" main=3570a182 train=08066ac9 2>&1 | rd "$here/$(echo "$s" | tr _ -)-head.txt"
done
{ c=3e073a96
  echo "# $c: $(git -C "$repo" show --format= -U0 "$c" | grep -c '^-[^-]') lines out, $(git -C "$repo" show --format= -U0 "$c" | grep -c '^+[^+]') in; the same once numbers are masked: $(diff <(git -C "$repo" show --format= -U0 "$c" | grep '^-[^-]' | cut -c2- | sed -E 's/:[0-9]+(-[0-9]+)?/:N/g' | sort) <(git -C "$repo" show --format= -U0 "$c" | grep '^+[^+]' | cut -c2- | sed -E 's/:[0-9]+(-[0-9]+)?/:N/g' | sort) > /dev/null && echo True || echo False)"
  echo "# each re-pointed number's line at $head:"
  for spec in 446-478 822 333-335 386-392 1121 1128 1301; do
    a=${spec%-*}
    echo "  scripts/fxcap/capture.sh:$spec  $(git -C "$repo" show "$head:scripts/fxcap/capture.sh" | sed -n "${a}p" | cut -c1-90)"
  done; } 2>&1 | rd "$here/repointed.txt"
