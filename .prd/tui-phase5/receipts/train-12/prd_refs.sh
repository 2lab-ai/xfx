#!/usr/bin/env bash
# Train 12: the `.prd/<file>:<line>` references cited from outside `.prd/`
# and outside the three files refs_prd.py reads (the ledger, docs/parity.md,
# capfile.py), at <head>; and whether any file they name changed between
# <main> and <head> (`git diff --stat`): a reference into an unchanged file
# cannot have moved.
# usage: prd_refs.sh <repo> <main> <head>
set -u
repo=$1 main=$2 head=$3
refs=$(git -C "$repo" grep -noE '(^|[^A-Za-z0-9_./-])\.prd/[A-Za-z0-9_./-]+\.(md|txt|sh|py|toml|tsv|json):[0-9]+(-[0-9]+)?' "$head" -- . ':!.prd' ':!docs/parity.md' ':!scripts/fxcap/parity-ledger.toml' ':!scripts/fxcap/capfile.py' | sed "s/^$head://")
echo "$refs" | sed -E 's/:[^:.]*\.prd\//: .prd\//; s/^/    /'
echo "# $(echo "$refs" | grep -c .) references; $(echo "$refs" | grep -vc '^docs/superpowers/plans/') outside the historical plans"
files=$(echo "$refs" | grep -oE '\.prd/[A-Za-z0-9_./-]+\.(md|txt|sh|py|toml|tsv|json)' | sort -u)
echo "# the files they name: $(echo "$files" | tr '\n' ' ')"
echo "# git diff --stat $main $head over them: $(git -C "$repo" diff --stat "$main" "$head" -- $files | tail -1 | grep . || echo 'no change')"
