#!/usr/bin/env bash
# Train 14: train 13's reruns.sh, unchanged but for this line ("main" is the base dcfa5146, main itself, here).
# Train 13 (train 12's reruns.sh, unchanged but for these lines; "main" is
# the base clone here): scenarios swept again, one hold of the suite lock,
# three times each on the head's binary and, where a base clone is given, on
# the base's (sweep-rerun.sh each, with that tree's own harness, scenarios
# and ledger). The brief's rule for a mover: three reruns before it is
# called a flake.
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
