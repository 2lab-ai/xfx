#!/usr/bin/env bash
# Train 12: the brief's path check at the receipt's last commit (train 11's
# redaction_grep.sh, read the brief's way):
#   1. each commit this train adds (first-parent, <base>..<head>: the four
#      merges, the re-point and the receipt's commits) as the brief reads it,
#      `git show --format= <c>`'s added lines -- a merge as git show prints
#      it, its combined diff -- every one 0; and each merge read as everything
#      it brings (`git diff <merge>^1 <merge>`, train 11's reading), counted;
#   2. midbranch_scan.py: every commit of <base>..<head>, the units' own
#      included, and the range itself, each hit classified (placeholder,
#      SCRATCH, NAME; a NAME stops the train);
#   3. every file of this directory, by the same pattern; round 2 adds
#      shapes.py's three shapes over it (the brief's, a run directory's name,
#      the harness root's uid tail);
#   4. round 2: each commit this train adds after <code-head>, and the files
#      it touches outside this directory (none: no code after the code head).
# The pattern is the brief's, each part spelt with a bracket class so this
# file does not match itself.
# usage: final-scan.sh <repo> <base> <head> [<code-head>]
set -u
repo=$1 base=$2 head=$3 code=${4:-}
here=$(cd "$(dirname "$0")" && pwd)
P='/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|zhugehyu[k]|claud[e]-[0-9]+/'
echo "## 1. each commit this train adds, first-parent $base..$(git -C "$repo" rev-parse --short=8 "$head"): the brief's count; a merge also as everything it brings"
mine=0
for c in $(git -C "$repo" rev-list --first-parent --reverse "$base..$head"); do
  n=$(git -C "$repo" show --format= "$c" | grep -E '^[+]' | grep -cE "$P")
  extra=""
  if [ "$(git -C "$repo" rev-list --parents -n1 "$c" | wc -w)" -gt 2 ]; then
    extra="; as everything it brings (git diff ^1): $(git -C "$repo" diff "$c^1" "$c" | grep -E '^[+]' | grep -cE "$P")"
  fi
  echo "$(git -C "$repo" rev-parse --short=8 "$c") $n$extra  $(git -C "$repo" log -1 --format=%s "$c" | cut -c1-70)"
  mine=$((mine + n))
done
echo "# the brief's count, summed over this train's commits: $mine"
echo
echo "## 2. every commit of the range, and the range (midbranch_scan.py)"
python3 -I -B "$here/midbranch_scan.py" "$repo" "$base" "$head"
rc=$?
echo "# midbranch_scan.py exit $rc"
echo
f=$(grep -lE "$P" "$here"/* | wc -l | tr -d ' ')
echo "## 3. files of this directory holding the pattern: $f"
python3 -I -B "$here/shapes.py" files "$here"/* | sed '1s/^/shapes.py over this directory: /'
sh=${PIPESTATUS[0]}
out=0
if [ -n "$code" ]; then
  echo
  echo "## 4. each commit after the code head $code, and the files it touches outside this directory"
  for c in $(git -C "$repo" rev-list --reverse "$code..$head"); do
    o=$(git -C "$repo" show --format= --name-only "$c" -- . ':!.prd/tui-phase5/receipts/train-12' | grep -c .)
    echo "$(git -C "$repo" rev-parse --short=8 "$c") outside: $o; inside: $(git -C "$repo" show --format= --name-only "$c" -- .prd/tui-phase5/receipts/train-12 | grep -c .) file(s)"
    out=$((out + o))
  done
  echo "# files outside this directory touched after the code head: $out"
fi
exit $(( mine + rc + f + sh + out > 0 ))
