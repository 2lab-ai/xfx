#!/usr/bin/env bash
# Train 11 (train 10's redaction_grep.sh, the brief's pattern whole): the
# brief's check, for each commit this train adds (first-parent,
# <base>..<head>) and for the range: the added lines that hold the home root,
# the harness temp roots, the macOS per-user temp root, the account's name or
# `claude-<n>/` -- each part spelt with a bracket class so this file does not
# match itself. A merge commit is read as everything it adds to the train
# (`git diff <merge>^1 <merge>`), not only its combined diff. The last term
# also matches a model path such as `models/claud[e]-3/`: every line it matches
# is listed, to be read. Then the same pattern over every file of this
# directory.
# usage: redaction_grep.sh <repo> <base> <head>
set -u
repo=$1 base=$2 head=$3
here=$(cd "$(dirname "$0")" && pwd)
P='/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|zhugehyu[k]|claud[e]-[0-9]+/'
LAST='claud[e]-[0-9]+/'
added() { # <commit>: its added lines
  if [ "$(git -C "$repo" rev-list --parents -n1 "$1" | wc -w)" -gt 2 ]; then
    git -C "$repo" diff "$1^1" "$1"
  else
    git -C "$repo" show --format= "$1"
  fi | grep -E '^[+]'
}
total=0 last=0
for c in $(git -C "$repo" rev-list --first-parent --reverse "$base..$head"); do
  n=$(added "$c" | grep -cE "$P")
  l=$(added "$c" | grep -cE "$LAST")
  echo "$(git -C "$repo" rev-parse --short=8 "$c") $n  $(git -C "$repo" log -1 --format=%s "$c" | cut -c1-70)"
  [ "$l" -eq 0 ] || added "$c" | grep -E "$LAST" | cut -c1-200 | sed 's/^/    last term: /'
  total=$((total + n)); last=$((last + l))
done
r=$(git -C "$repo" diff "$base..$head" | grep -E '^[+]' | grep -cE "$P")
echo "# range $base..$(git -C "$repo" rev-parse --short=8 "$head"): $r; the commits' sum: $total (of them the last term alone: $last)"
f=$(grep -lE "$P" "$here"/* | wc -l | tr -d ' ')
echo "# files of this directory holding the pattern: $f"
exit $(( r + total + f > 0 ))
