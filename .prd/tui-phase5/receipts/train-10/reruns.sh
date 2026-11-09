#!/usr/bin/env bash
# Train 10: the movers swept again, one hold of the suite lock: each scenario
# whose screen moved on xfx's side against main's sweep with no unit's change
# to explain it, three times on the head's binary, and model-parked three
# times on main's too (sweep-rerun.sh each).
# usage: FX=<fx v0.0.16> SHORT_TMP=<a short scratch dir> suite-lock.sh reruns.sh <head-clone> <main-clone> <out-dir>
set -u
head=$1 main=$2 out=$3
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out"
bash "$here/sweep-rerun.sh" head "$head" model-parked 3 "$out/head-model-parked"
bash "$here/sweep-rerun.sh" head "$head" workspace-index 3 "$out/head-workspace-index"
bash "$here/sweep-rerun.sh" main "$main" model-parked 3 "$out/main-model-parked"
