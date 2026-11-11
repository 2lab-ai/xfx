#!/usr/bin/env bash
# AT-RANK's sweep (CTRLO-AT-KEYS', AT-LAUNCH-WALK's, with this unit's --only list): the base's xfx and the head's against fx v0.0.16,
# one after the other, inside one hold of the suite lock
# (`suite-lock.sh sweeps.sh ...`), the same scenarios, ledger and size.
# sweep.py checks fx's sha256 before and after each run.
#
# usage: suite-lock.sh sweeps.sh <repo> <fx> <xfx base> <xfx head> <out dir>
set -euo pipefail
repo=$1 fx=$2 base=$3 head=$4 out=$5
pin=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
only=(--only 'at*' --only 'p5-input-at*' --only 'filepicker*' --only 'p5-ctrlo*' --only 'p5-at-rank' --only 'workspace-index')
for side in base head; do
  bin=$base
  [ "$side" = head ] && bin=$head
  start=$SECONDS
  "$repo/scripts/fxcap/sweep.sh" --fx "$fx" --pin-sha "$pin" --xfx "$bin" --size 40x120 --jobs 2 "${only[@]}" --out "$out/$side" > "$out/$side.log" 2>&1 || echo "# $side sweep exit $?"
  echo "# $side sweep: $((SECONDS - start)) s"
done
