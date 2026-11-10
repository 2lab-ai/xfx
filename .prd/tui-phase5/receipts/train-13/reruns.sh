#!/usr/bin/env bash
# Train 12 (train 11's reruns.sh, its scenarios given on the command line):
# scenarios swept again, one hold of the suite lock, three times each on the
# head's binary and, where a main clone is given, on main's (sweep-rerun.sh
# each, with that tree's own harness, scenarios and ledger). The brief's rule
# for a mover: three reruns before it is called a flake. model-column-idle is
# also rerun at the head whatever its verdict, for the brief's question of
# whether SWEEP-WALLCLOCK-F ended train 11's `catalog-window`.
# usage: FX=<fx v0.0.16> SHORT_TMP=<a short scratch dir> \
#        suite-lock.sh reruns.sh <head-clone> <main-clone|-> <out-dir> <scenario>...
set -u
head=$1 main=$2 out=$3; shift 3
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out"
for s in "$@"; do
  bash "$here/sweep-rerun.sh" head "$head" "$s" 3 "$out/head-$s"
  [ "$main" = - ] || bash "$here/sweep-rerun.sh" main "$main" "$s" 3 "$out/main-$s"
done
