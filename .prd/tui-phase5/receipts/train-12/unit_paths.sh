#!/usr/bin/env bash
# Train 12, join checks 3 and 4: each unit's receipt folder at <head>, read by
# `git grep` (tracked files only): how many files it holds, how many hold the
# brief's pattern (each part spelt with a bracket class so this file does not
# match itself) or one of capture.sh's placeholders (`<work>`, `<scratch___>`,
# `<tmp:real>`, ...), and how many times it names a run directory (`fxcap.`
# or `fxsw.` and six name characters: what capture.sh's rewrite and the
# ledger's `run-directory` exist for), the masked template `XXXXXX` apart;
# then every file the pattern holds, for the reader.
# check-no-local-paths.sh is the gate; this only counts, per unit.
# usage: unit_paths.sh <repo> <head> <receipt-dir>...
set -u
repo=$1 head=$2; shift 2
P='/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|zhugehyu[k]|claud[e]-[0-9]+/'
count() { git -C "$repo" grep -lE "$1" "$head" -- "$2" | wc -l | tr -d ' '; }
for d in "$@"; do
  n=$(git -C "$repo" ls-tree -r --name-only "$head" "$d" | wc -l | tr -d ' ')
  # round 2: counted, not named (a run directory's name is a scan's shape)
  runs=$(git -C "$repo" grep -hoE 'fxca[p]\.[A-Za-z0-9]{6}|fxs[w]\.[A-Za-z0-9_]{6}' "$head" -- "$d" | grep -vc 'XXXXXX$')
  echo "$d: $n files; the brief's pattern $(count "$P" "$d"); a placeholder $(count '<(work|scratch|tmp|home)(:real)?_*>' "$d"); run-directory names other than the template: $runs"
  git -C "$repo" grep -cE "$P" "$head" -- "$d" | sed "s/^$head://; s/^/    the pattern: /"
done
