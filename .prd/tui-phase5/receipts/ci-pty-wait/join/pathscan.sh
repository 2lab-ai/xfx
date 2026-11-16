#!/bin/bash
# The brief's path check over every commit this branch holds that train 16
# does not (`git rev-list HEAD --not 8be4693b`: this unit's commits, its
# earlier joins with main, and this join), each commit's message and the
# lines `git show` prints of it (for a merge, its combined diff: what the
# merge itself wrote), and then over every file of this folder on disk. The
# pattern is train 16's pathcheck.sh's (the brief's eight terms), each part
# spelt with a bracket class so this file does not match itself.
#   pathscan.sh <repo>
# Prints one line per commit with a hit, the counts, and every file hit.
set -u
cd "$1" || exit 2
P='/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|zhugehyu[k]|claud[e]-[0-9]+/|-[0-9]{3}/[t][0-9]+/|fx[s][w]\.[A-Za-z0-9_]{6}'
commits=0 hits=0
for c in $(git rev-list HEAD --not 8be4693b); do
  commits=$((commits + 1))
  n=$({ git log -1 --format=%B "$c"; git show --format= "$c"; } | grep -cE "$P")
  if [ "$n" -gt 0 ]; then echo "$(git log -1 --format='%h %s' "$c" | cut -c1-70): $n"; hits=$((hits + n)); fi
done
echo "commits $commits (through HEAD, \"$(git log -1 --format=%s HEAD | cut -c1-60)\"; not in 8be4693b): $hits matching lines"
files=$(grep -rlE "$P" .prd/tui-phase5/receipts/ci-pty-wait | wc -l | tr -d ' ')
grep -rlE "$P" .prd/tui-phase5/receipts/ci-pty-wait
echo "this folder on disk: $files files matching"
[ "$hits" -eq 0 ] && [ "$files" -eq 0 ]
