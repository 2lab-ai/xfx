#!/usr/bin/env bash
# Train 16 (new): the brief's mid-branch check before any merge -- the
# units' own commits, each unit stack's range from its merge base with this
# train's base (866747ff, train 15's head) to its top, read by
# midbranch_scan.py (every commit as `git show --format=` prints it, a merge
# as its combined diff, every hit classified; a NAME or a CREDENTIAL stops
# the train), then each commit message of the range by the brief's pattern
# (each part spelt with a bracket class so this file does not match itself),
# counted, the commit named, the text never printed. The documented homes
# are read from the train base's check-no-local-paths.sh for every range.
# usage: prescan.sh <repo> <train-base>
set -u
repo=$1 tb=$2
here=$(cd "$(dirname "$0")" && pwd)
P='/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|zhugehyu[k]|claud[e]-[0-9]+/|-[0-9]{3}/[t][0-9]+/|fx[s][w]\.[A-Za-z0-9_]{6}'
worst=0
for spec in profile=bacb1052 slash=32eb858f test-env-isolation=7c3f6c1e local-paths-wrap=dd9f5f0a approval-paste-spaces=ca496c10 meta-prefix-decode=6d94fbc0; do
  name=${spec%%=*} tip=${spec#*=}
  mb=$(git -C "$repo" merge-base "$tb" "$tip")
  echo "## $name: $(git -C "$repo" rev-parse --short=8 "$mb")..$tip, $(git -C "$repo" rev-list --count "$mb..$tip") commits ($(git -C "$repo" rev-list --merges --count "$mb..$tip") merge)"
  python3 -I -B "$here/midbranch_scan.py" "$repo" "$mb" "$tip" "$tb"
  rc=$?
  echo "# midbranch_scan.py exit $rc"
  [ "$rc" -eq 0 ] || worst=1
  m=0
  for c in $(git -C "$repo" rev-list "$mb..$tip"); do
    k=$(git -C "$repo" log -1 --format=%B "$c" | grep -cE "$P")
    [ "$k" -eq 0 ] || echo "    message: $(git -C "$repo" rev-parse --short=8 "$c") $k line(s)"
    m=$((m + k))
  done
  echo "# commit messages: $m line(s) holding the brief's pattern"
  echo
done
exit "$worst"
