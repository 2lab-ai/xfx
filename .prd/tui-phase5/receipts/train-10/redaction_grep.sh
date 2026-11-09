#!/usr/bin/env bash
# Train 10: the brief's check, for each commit this train adds (first-parent,
# <base>..<head>) and for the range: the added lines of `git show`/`git diff`
# that hold the home root, the harness temp roots, the macOS per-user temp
# root or the account's name -- the brief's pattern, each part spelt with a
# bracket class so this file does not match itself. Then the same pattern over
# every file of this directory.
# usage: redaction_grep.sh <repo> <base> <head>
set -u
repo=$1 base=$2 head=$3
here=$(cd "$(dirname "$0")" && pwd)
P='/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|zhugehyu[k]'
total=0
for c in $(git -C "$repo" rev-list --first-parent --reverse "$base..$head"); do
  n=$(git -C "$repo" show --format= "$c" | grep -E '^[+]' | grep -cE "$P")
  echo "$(git -C "$repo" rev-parse --short=8 "$c") $n  $(git -C "$repo" log -1 --format=%s "$c" | cut -c1-70)"
  total=$((total + n))
done
r=$(git -C "$repo" diff "$base..$head" | grep -E '^[+]' | grep -cE "$P")
echo "# range $base..$(git -C "$repo" rev-parse --short=8 "$head"): $r; the commits' sum: $total"
f=$(grep -lE "$P" "$here"/* | wc -l | tr -d ' ')
echo "# files of this directory holding the pattern: $f"
exit $(( r + total + f > 0 ))
