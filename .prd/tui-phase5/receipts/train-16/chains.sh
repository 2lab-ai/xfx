#!/usr/bin/env bash
# Train 16 (new): each merge's unit chain, derived with git log -- the
# merge base of the train's base and the stack top, every commit of the
# range that a branch points at (the units' tips, and their review refs),
# each with its count since the base; then where PERMISSION-MODE-CASE
# stands (the brief listed it on the slash line).
# usage: chains.sh <repo> <train-base>
set -u
repo=$1 tb=$2
for top in bacb1052 32eb858f 7c3f6c1e dd9f5f0a ca496c10 6d94fbc0; do
  mb=$(git -C "$repo" merge-base "$tb" "$top")
  echo "## $top: base $(git -C "$repo" rev-parse --short=8 "$mb"), $(git -C "$repo" rev-list --count "$mb..$top") commits, $(git -C "$repo" rev-list --merges --count "$mb..$top") merge(s)"
  for c in $(git -C "$repo" rev-list --reverse "$mb..$top"); do
    refs=$(git -C "$repo" for-each-ref --points-at "$c" --format='%(refname:short)' refs/heads | tr '\n' ' ')
    [ -n "$refs" ] && echo "  $(git -C "$repo" rev-parse --short=8 "$c") $(git -C "$repo" rev-list --count "$mb..$c") $refs"
  done
done
echo "## PERMISSION-MODE-CASE f74d2fd5"
for top in bacb1052 32eb858f; do
  git -C "$repo" merge-base --is-ancestor f74d2fd5 "$top" && echo "  an ancestor of $top" || echo "  not an ancestor of $top"
done
