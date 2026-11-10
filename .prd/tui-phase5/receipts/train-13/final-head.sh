#!/usr/bin/env bash
# Train 13 round 2 (gpt6 receipt MUST 1): what changed after the code head,
# read at <rev>: `git show --stat` of every commit of <code-head>..<rev>
# (oldest first); then `git diff --stat <code-head> <rev>` outside this
# folder; then, for each file there under `src/`, where its hunks fall
# against the line that opens its test module (`mod tests {`) at <rev>.
# usage: final-head.sh <repo> <code-head> <rev>
set -u
repo=$1 code=$2 rev=$3
F=.prd/tui-phase5/receipts/train-13
echo "# read at $(git -C "$repo" rev-parse --short=8 "$rev"); code head $code"
echo "## git show --stat, every commit of $code..$(git -C "$repo" rev-parse --short=8 "$rev")"
for c in $(git -C "$repo" rev-list --reverse "$code..$rev"); do
  git -C "$repo" show --stat=200 --format='%h %s' "$c" | sed '/^$/d'
done
echo "## git diff --stat $code $(git -C "$repo" rev-parse --short=8 "$rev"), outside $F"
git -C "$repo" diff --stat=200 "$code" "$rev" -- . ":!$F" | sed 's/^/    /'
for f in $(git -C "$repo" diff --name-only "$code" "$rev" -- src); do
  t=$(git -C "$repo" show "$rev:$f" | grep -n '^mod tests {' | cut -d: -f1)
  echo "## $f: its test module opens at :$t at $(git -C "$repo" rev-parse --short=8 "$rev"); each hunk's lines there"
  git -C "$repo" diff -U0 "$code" "$rev" -- "$f" | grep '^@@' | while read -r _ old new _; do
    a=${new#+}; s=${a%%,*}; n=${a#*,}; [ "$n" = "$a" ] && n=1
    echo "    $old $new: new lines $s-$((s + n - 1)), $([ "$s" -gt "$t" ] && echo 'inside the test module' || echo 'OUTSIDE the test module')"
  done
done
