#!/usr/bin/env bash
# Train 11 (train 10's reruns.sh, its scenarios this train's): the movers
# swept again, one hold of the suite lock: model-column-idle (passes on main's
# and CATALOG-REFRESH's own sweep, not on the head's) and model-parked (the
# other way), three times each on the head's binary and on main's
# (sweep-rerun.sh each). Both scenarios' catalogs are down for a wall-clock
# window from the fake Gateway's start (`modelsdown 9000`, `modelsdown 6000`).
# usage: FX=<fx v0.0.16> SHORT_TMP=<a short scratch dir> suite-lock.sh reruns.sh <head-clone> <main-clone> <out-dir>
set -u
head=$1 main=$2 out=$3
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out"
for s in model-column-idle model-parked; do
  bash "$here/sweep-rerun.sh" head "$head" "$s" 3 "$out/head-$s"
  bash "$here/sweep-rerun.sh" main "$main" "$s" 3 "$out/main-$s"
done
